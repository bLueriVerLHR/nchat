use std::{
    net::{Ipv4Addr, SocketAddr, UdpSocket},
    str::from_utf8,
    sync::mpsc::{channel, Receiver, Sender},
    thread::{self},
};

use chrono::{DateTime, Local};
use clap::Parser;
use cursive::{
    event,
    view::{Nameable, Resizable, Scrollable},
    views::{Dialog, EditView, LinearLayout, TextView},
    Cursive, CursiveRunnable, CursiveRunner,
};
use nchat::{ControlCode, Group, Member, Message};

#[derive(Parser, Debug)]
pub struct Args {
    /// client will use <ADDRESS> for udp send/receive
    #[arg(short, long, default_value_t = SocketAddr::from((Ipv4Addr::LOCALHOST, 9090)))]
    address: SocketAddr,

    /// server address
    #[arg(short, long, default_value_t = SocketAddr::from((Ipv4Addr::LOCALHOST, 8080)))]
    server: SocketAddr,

    /// default login channel
    #[arg(short, long, default_value_t = String::from("global"))]
    group: String,

    /// nickname
    #[arg(short, long, default_value_t = String::from("unknow"))]
    nickname: String,
}

struct InternalMessage {
    code: ControlCode,
    msg: String,
}

struct Client {
    nickname: String,
    group: String,
    socket: UdpSocket,
}

impl Client {
    pub fn new(nickname: String, address: SocketAddr, server: SocketAddr) -> Client {
        let socket = UdpSocket::bind(&address).unwrap();
        socket.connect(server).unwrap();
        Client {
            nickname,
            group: String::new(),
            socket,
        }
    }

    pub fn clone_socket(&self) -> UdpSocket {
        self.socket.try_clone().unwrap()
    }

    pub fn current_group(&self) -> &String {
        &self.group
    }

    pub fn get_address(&self) -> SocketAddr {
        self.socket.local_addr().unwrap()
    }

    pub fn get_group(&self) -> Group {
        Group::new(self.group.clone(), 0)
    }

    pub fn get_member(&self) -> Member {
        Member::new(self.nickname.clone(), self.socket.local_addr().unwrap())
    }

    fn try_login(&mut self, group: &String) {
        let g = self.get_group();
        let m = self.get_member();
        let msg = Message::new_default(ControlCode::JoinGroup, g, m, group.clone());
        let buf = serde_json::to_string(&msg).unwrap();
        self.socket
            .send(buf.as_bytes())
            .expect("login info send failed");
        self.group = group.clone();
    }
}

fn main() {
    let args = Args::parse();
    let mut client = Client::new(args.nickname, args.address, args.server);
    client.try_login(&args.group);

    // set mailbox for receiving message from server
    let (mail_sender, mail_receiver) = channel();
    let udp = client.clone_socket();
    let addr = client.get_address();
    let mailbox = thread::spawn(move || loop {
        let mut buf = [0; 4096];
        let recv = udp.recv(&mut buf);
        if recv.is_err() {
            continue;
        } else {
            let raw = from_utf8(&buf[..recv.unwrap()]).unwrap();
            let msg: Message = serde_json::from_str(&raw).unwrap();
            let exit_server = match msg.get_code() {
                ControlCode::EixtServer => {
                    if msg.get_sender().get_address() == &addr {
                        true
                    } else {
                        false
                    }
                }
                _ => false,
            };
            mail_sender.send(msg).unwrap();
            if exit_server {
                break;
            }
        }
    });

    // render message in the backgroud
    let (view_sender, view_receiver) = channel::<TextView>();
    let message_render = thread::spawn(move || {
        for m in mail_receiver.iter() {
            let sender = m.get_sender();
            let timestamp = m.get_timestamp();
            let dt = DateTime::from_timestamp(timestamp, 0).unwrap();
            let loc_dt = dt.with_timezone(&Local);
            let msg = m.get_msg();
            let text = match m.get_code() {
                ControlCode::Error => {
                    format!("## server send an error: {}", msg)
                }
                ControlCode::EixtServer => {
                    format!(
                        "👋 {}@{} has exit the server -- {}",
                        sender.get_nickname(),
                        sender.get_address(),
                        loc_dt,
                    )
                }
                ControlCode::JoinGroup => {
                    format!(
                        "😊 {}@{} has joined the group -- {}",
                        sender.get_nickname(),
                        sender.get_address(),
                        loc_dt,
                    )
                }
                ControlCode::LeaveGroup => {
                    format!(
                        "👋 {}@{} has left the group -- {}",
                        sender.get_nickname(),
                        sender.get_address(),
                        loc_dt,
                    )
                }
                ControlCode::SendMessage => {
                    format!(
                        "~> {}@{} -- {} <~\n{}",
                        sender.get_nickname(),
                        sender.get_address(),
                        loc_dt,
                        msg
                    )
                }
            };
            view_sender.send(TextView::new(text)).unwrap();

            match m.get_code() {
                ControlCode::EixtServer => {
                    if m.get_sender().get_address() == &addr {
                        break;
                    }
                }
                _ => {}
            }
        }
    });

    // set postman for sending message to server
    let (post_sender, post_receiver) = channel::<InternalMessage>();
    let udp = client.clone_socket();
    let m = client.get_member();
    let g = client.get_group();
    let postman = thread::spawn(move || {
        let mut default = Message::new_default(ControlCode::SendMessage, g, m, String::new());
        for imsg in post_receiver.iter() {
            default.set_code(imsg.code);
            default.set_msg(imsg.msg);
            let buf = serde_json::to_string(&default).unwrap();
            udp.send(buf.as_bytes()).unwrap();

            match default.get_code() {
                ControlCode::EixtServer => {
                    break;
                }
                _ => {}
            }
        }
    });

    let mut siv = default_window();
    render(&mut siv, client.current_group(), post_sender.clone());
    let mut siv = siv.runner();
    run(&mut siv, view_receiver, post_sender);

    // waiting for leave message sent out
    postman.join().unwrap();
    mailbox.join().unwrap();
    message_render.join().unwrap();
}

fn default_window() -> CursiveRunnable {
    let mut siv = cursive::default();
    siv.add_global_callback(event::Key::Esc, move |s| {
        s.quit();
    });
    siv.add_global_callback(event::Event::CtrlChar('c'), move |s| {
        s.quit();
    });
    siv.add_global_callback(event::Event::CtrlChar('d'), move |s| {
        s.quit();
    });
    siv.add_global_callback(event::Key::Del, |s| {
        s.call_on_name("chat.edit", |v: &mut EditView| {
            v.set_content("");
        })
        .unwrap();
    });
    siv.add_global_callback(event::Event::Alt(event::Key::Del), |s| {
        s.call_on_name("chat.history", |v: &mut LinearLayout| {
            v.clear();
        })
        .unwrap();
    });
    return siv;
}

fn render(siv: &mut CursiveRunnable, title: &String, sender: Sender<InternalMessage>) {
    let history = LinearLayout::vertical()
        .with_name("chat.history")
        .full_width()
        .full_height()
        .scrollable();

    let editor = EditView::new()
        .on_submit(move |_s, text| {
            let imsg = InternalMessage {
                code: ControlCode::SendMessage,
                msg: text.to_string(),
            };
            sender.send(imsg).unwrap();
        })
        .with_name("chat.edit")
        .full_width();

    let chat_view = Dialog::around(LinearLayout::vertical().child(history).child(editor))
        .title(title)
        .with_name("chat.win");

    siv.add_fullscreen_layer(chat_view);
}

fn run(
    siv: &mut CursiveRunner<&mut Cursive>,
    receiver: Receiver<TextView>,
    sender: Sender<InternalMessage>,
) {
    let mut msg_cnt: u64 = 0;
    siv.refresh();
    loop {
        siv.step();
        if !siv.is_running() {
            let imsg = InternalMessage {
                code: ControlCode::EixtServer,
                msg: String::new(),
            };
            sender.send(imsg).unwrap();
            break;
        }

        let mut needs_refresh = false;
        for text_view in receiver.try_iter() {
            needs_refresh = true;
            msg_cnt += 1;
            siv.call_on_name("chat.history", move |v: &mut LinearLayout| {
                v.add_child(text_view);
            });
        }

        if needs_refresh {
            siv.refresh();
        }
    }
    println!("receive {} messages in this session", msg_cnt);
}

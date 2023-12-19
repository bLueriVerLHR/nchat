use std::{
    net::{Ipv4Addr, SocketAddr, UdpSocket},
    str::from_utf8,
    sync::mpsc::{channel, Receiver, TryIter},
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
    /// client will use 127.0.0.1:<PORT> for udp send/receive
    #[arg(short, long, default_value_t = 9090)]
    port: u16,

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

struct Client {
    nickname: String,
    group: String,
    socket: UdpSocket,
    receiver: Receiver<Message>,
}

impl Client {
    pub fn new(
        nickname: String,
        port: u16,
        server: SocketAddr,
        receiver: Receiver<Message>,
    ) -> Client {
        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
        let socket = UdpSocket::bind(&address).unwrap();
        socket.connect(server).unwrap();
        Client {
            nickname,
            group: String::new(),
            socket,
            receiver,
        }
    }

    pub fn try_iter(&self) -> TryIter<'_, Message> {
        self.receiver.try_iter()
    }

    pub fn clone_socket(&self) -> UdpSocket {
        self.socket.try_clone().unwrap()
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
    let (msg_sender, msg_receiver) = channel();
    let mut client = Client::new(args.nickname, args.port, args.server, msg_receiver);
    client.try_login(&args.group);
    let udp = client.clone_socket();
    let _mailbox = thread::spawn(move || loop {
        let mut buf = [0; 4096];
        let recv = udp.recv(&mut buf);
        if recv.is_err() {
            continue;
        } else {
            let raw = from_utf8(&buf[..recv.unwrap()]).unwrap();
            let msg: Message = serde_json::from_str(&raw).unwrap();
            msg_sender.send(msg).unwrap();
        }
    });

    let mut siv = cursive::default();
    add_callbacks(&mut siv, &client);
    render(&mut siv, &client);
    let mut siv = siv.runner();
    run(&mut siv, &client);
}

fn add_callbacks(siv: &mut CursiveRunnable, client: &Client) {
    let socket = client.clone_socket();
    let g = client.get_group();
    let m = client.get_member();
    siv.add_global_callback(event::Key::Esc, move |s| {
        let msg = Message::new_default(
            ControlCode::LeaveGroup,
            g.clone(),
            m.clone(),
            String::from("global"),
        );
        let buf = serde_json::to_string(&msg).unwrap();
        socket.send(buf.as_bytes()).unwrap();
        s.quit();
    });
    siv.add_global_callback(event::Key::Del, |s| {
        s.call_on_name("chat.edit", |v: &mut EditView| {
            v.set_content("");
        })
        .unwrap();
    });
}

fn render(siv: &mut CursiveRunnable, client: &Client) {
    let g = client.get_group();
    let m = client.get_member();
    let socket = client.clone_socket();

    siv.set_window_title("nchat");

    let history = LinearLayout::vertical()
        .with_name("chat.history")
        .full_width()
        .full_height()
        .scrollable();

    let editor = EditView::new()
        .on_submit(move |_s, text| {
            if text.is_empty() {
                return;
            }
            let msg = Message::new_default(
                ControlCode::SendMessage,
                g.clone(),
                m.clone(),
                text.to_string(),
            );
            let buf = serde_json::to_string(&msg).unwrap();
            socket.send(buf.as_bytes()).unwrap();
        })
        .with_name("chat.edit")
        .full_width();

    let chat_view =
        Dialog::around(LinearLayout::vertical().child(history).child(editor)).title("global");

    siv.add_fullscreen_layer(chat_view)
}

fn run(siv: &mut CursiveRunner<&mut Cursive>, client: &Client) {
    let mut msg_cnt: u64 = 0;
    siv.refresh();
    loop {
        siv.step();
        if !siv.is_running() {
            break;
        }

        let mut needs_refresh = false;
        for m in client.try_iter() {
            let sender = m.get_sender();
            let timestamp = m.get_timestamp();
            let dt = DateTime::from_timestamp(timestamp, 0).unwrap();
            let loc_dt = dt.with_timezone(&Local);
            let msg = m.get_msg();
            let text = match m.get_code() {
                ControlCode::Error => {
                    format!("## server send an error: {}", msg)
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
            siv.call_on_name("chat.history", |v: &mut LinearLayout| {
                needs_refresh = true;
                msg_cnt += 1;
                // while msg_cnt >= 32 {
                //     v.remove_child(0);
                //     msg_cnt -= 1;
                // }
                v.add_child(TextView::new(text));
            });
        }

        if needs_refresh {
            siv.refresh();
        }
    }
}

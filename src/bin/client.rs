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

#[derive(Clone)]
enum ClientCode {
    SendMessage,
    ReceiveMessage,
    ClientShutdown,
}

#[derive(Clone)]
struct InternalMessage {
    code: ClientCode,
    msg: String,
}

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

impl InternalMessage {
    pub fn new(code: ClientCode, msg: String) -> InternalMessage {
        InternalMessage { code, msg }
    }

    pub fn get_code(&self) -> &ClientCode {
        &self.code
    }

    pub fn get_message(&self) -> &String {
        &self.msg
    }
}

struct Client {
    nickname: String,
    group: String,
    socket: UdpSocket,
}

impl Client {
    pub fn new(nickname: String, socket: UdpSocket) -> Client {
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

    pub fn get_group(&self) -> Group {
        Group::new(self.group.clone(), 0)
    }

    pub fn get_member(&self) -> Member {
        Member::new(self.nickname.clone(), self.socket.local_addr().unwrap())
    }

    fn try_login(&mut self, group: &str) {
        let g = self.get_group();
        let m = self.get_member();
        let msg = Message::new_default(ControlCode::JoinGroup, g, m, group.to_owned());
        let buf = serde_json::to_string(&msg).unwrap();
        self.socket
            .send(buf.as_bytes())
            .expect("login info send failed");
        self.group = group.to_owned();
    }
}

fn main() {
    let args = Args::parse();
    let socket = UdpSocket::bind(args.address).unwrap();
    socket.connect(args.server).unwrap();
    let mut client = Client::new(args.nickname, socket);
    client.try_login(&args.group);

    // set mailbox for receiving message from server
    let (mail_sender, mail_receiver) = channel();
    let socket = client.clone_socket();
    let boxmail_sender = mail_sender.clone();
    let mailbox = thread::spawn(move || {
        let mut buf = [0; 4096];
        while forward_udp(&mut buf, &socket, &boxmail_sender) {}
    });

    // render message in the backgroud
    let (view_sender, view_receiver) = channel::<TextView>();
    let prerender = thread::spawn(move || {
        forward_prerender(&mail_receiver, &view_sender);
    });

    // set postman for sending message to server
    let (post_sender, post_receiver) = channel::<InternalMessage>();
    let socket = client.clone_socket();
    let m = client.get_member();
    let g = client.get_group();
    let postman = thread::spawn(move || {
        forward_client_message(&post_receiver, &m, &g, &socket);
    });

    let mut siv = default_window();
    render(&mut siv, client.current_group(), post_sender.clone());
    let mut siv = siv.runner();
    run(&mut siv, view_receiver, vec![mail_sender, post_sender]);

    // waiting for leave message sent out
    prerender.join().unwrap();
    postman.join().unwrap();
    mailbox.join().unwrap();
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
    siv
}

fn render(siv: &mut CursiveRunnable, title: &String, sender: Sender<InternalMessage>) {
    let history = LinearLayout::vertical()
        .with_name("chat.history")
        .full_width()
        .full_height()
        .scrollable();

    let editor = EditView::new()
        .on_submit(move |_s, text| {
            let imsg = InternalMessage::new(ClientCode::SendMessage, text.to_string());
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
    senders: Vec<Sender<InternalMessage>>,
) {
    let mut msg_cnt: u64 = 0;
    siv.refresh();
    loop {
        siv.step();
        if !siv.is_running() {
            // broadcast the shutdown message
            let imsg = InternalMessage::new(ClientCode::ClientShutdown, String::new());
            for sender in senders.iter() {
                sender.send(imsg.clone()).unwrap();
            }
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

fn forward_udp(buf: &mut [u8], socket: &UdpSocket, sender: &Sender<InternalMessage>) -> bool {
    loop {
        let len = match socket.recv(buf) {
            Ok(len) => len,
            Err(err) => {
                // bad but sometime useful exit
                println!("{}", err);
                return false;
            }
        };
        let raw = match from_utf8(&buf[..len]) {
            Ok(utf8str) => utf8str,
            Err(err) => {
                println!("{}", err);
                continue;
            }
        };
        let imsg = InternalMessage::new(ClientCode::ReceiveMessage, raw.to_string());
        match sender.send(imsg) {
            Ok(()) => {
                let msg: Message = match serde_json::from_str(raw) {
                    Ok(msg) => msg,
                    Err(err) => {
                        println!("{}", err);
                        continue;
                    }
                };
                match msg.get_code() {
                    ControlCode::EixtServer => return false,
                    _ => return true,
                }
            }
            Err(err) => {
                // bad but sometime useful exit
                println!("{}", err);
                return false;
            }
        };
    }
}

fn forward_prerender(receiver: &Receiver<InternalMessage>, sender: &Sender<TextView>) {
    for imsg in receiver.iter() {
        let m: Message = match imsg.code {
            ClientCode::ReceiveMessage => match serde_json::from_str(imsg.get_message()) {
                Ok(msg) => msg,
                Err(err) => {
                    println!("{}", err);
                    continue;
                }
            },
            ClientCode::ClientShutdown => {
                break;
            }
            _ => {
                println!("unexpected message received");
                continue;
            }
        };
        match message_prerender(m) {
            Some(m) => sender.send(TextView::new(m)).unwrap(),
            None => continue,
        }
    }
}

fn forward_client_message(
    receiver: &Receiver<InternalMessage>,
    member: &Member,
    group: &Group,
    socket: &UdpSocket,
) {
    let mut default = Message::new_default(
        ControlCode::SendMessage,
        group.clone(),
        member.clone(),
        String::new(),
    );
    for imsg in receiver.iter() {
        match imsg.get_code() {
            ClientCode::ClientShutdown => default.set_code(ControlCode::EixtServer),
            ClientCode::SendMessage => default.set_code(ControlCode::SendMessage),
            _ => {
                continue;
            }
        }
        default.set_message(imsg.get_message().clone());
        let buf = serde_json::to_string(&default).unwrap();
        socket.send(buf.as_bytes()).unwrap();

        match imsg.get_code() {
            ClientCode::ClientShutdown => {
                break;
            }
            _ => {}
        }
    }
}

fn message_prerender(m: Message) -> Option<String> {
    let from = m.get_sender();
    let timestamp = m.get_timestamp();
    let msg = m.get_message();
    let code = m.get_code();
    let datetime = match DateTime::from_timestamp(timestamp, 0) {
        Some(ts) => ts,
        None => {
            println!("timestamp convert failed");
            return None;
        }
    };
    let local_datetime = datetime.with_timezone(&Local);
    let text = match code {
        ControlCode::Error => {
            format!("## server send an error: {}", msg)
        }
        ControlCode::EixtServer => {
            format!(
                "👋 {}@{} has exit the server -- {}",
                from.get_nickname(),
                from.get_address(),
                local_datetime,
            )
        }
        ControlCode::JoinGroup => {
            format!(
                "😊 {}@{} has joined the group -- {}",
                from.get_nickname(),
                from.get_address(),
                local_datetime,
            )
        }
        ControlCode::LeaveGroup => {
            format!(
                "👋 {}@{} has left the group -- {}",
                from.get_nickname(),
                from.get_address(),
                local_datetime,
            )
        }
        ControlCode::SendMessage => {
            format!(
                "~> {}@{} -- {} <~\n{}",
                from.get_nickname(),
                from.get_address(),
                local_datetime,
                msg
            )
        }
    };
    Some(text)
}

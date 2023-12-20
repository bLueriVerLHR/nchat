use std::collections::HashSet;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::str::from_utf8;

use clap::Parser;
use nchat::{ControlCode, Message};

#[derive(Parser, Debug)]
pub struct Args {
    /// server will listen on <LOCAL>
    #[arg(short, long, default_value_t = SocketAddr::from((Ipv4Addr::LOCALHOST, 8080)))]
    address: SocketAddr,
}

pub struct Server {
    groups: HashSet<String>,
    members: HashSet<SocketAddr>,
    socket: UdpSocket,
}

impl Server {
    pub fn new(address: SocketAddr) -> Server {
        let socket = UdpSocket::bind(address).unwrap();
        println!("server will listen on {}", address);
        let mut server = Server {
            groups: HashSet::new(),
            members: HashSet::new(),
            socket,
        };
        server.add_group(String::from("global"));
        server
    }

    fn group_exist(&mut self, name: &String) -> bool {
        self.groups.contains(name)
    }

    fn add_group(&mut self, name: String) -> bool {
        self.groups.insert(name)
    }

    fn add_member(&mut self, addr: SocketAddr) -> bool {
        self.members.insert(addr)
    }

    fn remove_member(&mut self, addr: &SocketAddr) -> bool {
        self.members.remove(&addr)
    }

    fn send_to_all(&mut self, msg: &Message) {
        let buf = serde_json::to_string(&msg).unwrap();
        for member in &self.members {
            self.socket.send_to(buf.as_bytes(), member).unwrap();
        }
    }

    fn send_to(&mut self, msg: &Message, src: &SocketAddr) {
        let buf = serde_json::to_string(&msg).unwrap();
        self.socket.send_to(buf.as_bytes(), src).unwrap();
    }

    pub fn listen(&mut self) {
        let mut recv_buf = [0; 4096];
        loop {
            let (amt, src) = self.socket.recv_from(&mut recv_buf).unwrap();
            self.parse_msg(&recv_buf[..amt], src);
        }
    }

    fn parse_msg(&mut self, raw: &[u8], src: SocketAddr) {
        let utf8msg = from_utf8(raw).unwrap();
        println!("{}", utf8msg);

        let msg: Message = serde_json::from_str(utf8msg).unwrap();
        match msg.get_code() {
            ControlCode::Error => self.handle_msg_error(msg, src),
            ControlCode::SendMessage => self.handle_msg_send_message(msg, src),
            ControlCode::JoinGroup => self.handle_msg_join_group(msg, src),
            ControlCode::LeaveGroup => self.handle_msg_leave_group(msg, src),
        };
    }

    fn handle_msg_error(&mut self, _msg: Message, _src: SocketAddr) {
        // simple ignore the message
    }

    fn handle_msg_send_message(&mut self, mut msg: Message, src: SocketAddr) {
        // simple send the message to all members
        msg.update_sender_address(src);
        msg.update_timestamp();
        self.send_to_all(&msg);
    }

    fn handle_msg_join_group(&mut self, mut msg: Message, src: SocketAddr) {
        // add the new member to the server
        if !self.group_exist(msg.get_msg()) {
            msg.set_msg(format!("group {} not exist", msg.get_msg()));
            msg.set_code(ControlCode::Error);
            msg.update_timestamp();
            self.send_to(&msg, &src);
            return;
        }
        self.add_member(src);
        msg.update_sender_address(src);
        msg.update_timestamp();
        self.send_to_all(&msg);
    }

    fn handle_msg_leave_group(&mut self, mut msg: Message, src: SocketAddr) {
        // remove the member from the server
        // TODO: remove all expire members
        self.remove_member(&src);
        msg.update_sender_address(src);
        msg.update_timestamp();
        self.send_to_all(&msg);
    }
}

fn main() {
    let args = Args::parse();
    let mut s = Server::new(args.address);
    s.listen();
}

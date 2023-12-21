use std::net::SocketAddr;

use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub enum ControlCode {
    SendMessage,
    JoinGroup,
    LeaveGroup,
    EixtServer,
    Error,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Member {
    nickname: String,
    address: SocketAddr,
}

impl Member {
    pub fn new(nickname: String, address: SocketAddr) -> Member {
        Member { nickname, address }
    }

    pub fn get_nickname(&self) -> &String {
        &self.nickname
    }

    pub fn set_nickname(&mut self, nickname: String) {
        self.nickname = nickname;
    }

    pub fn get_address(&self) -> &SocketAddr {
        &self.address
    }

    pub fn set_address(&mut self, address: SocketAddr) {
        self.address = address;
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Group {
    name: String,
    id: u64,
}

impl Group {
    pub fn new(name: String, id: u64) -> Group {
        Group { name, id }
    }

    pub fn get_name(&self) -> &String {
        &self.name
    }

    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }

    pub fn get_id(&self) -> u64 {
        self.id
    }

    pub fn set_id(&mut self, id: u64) {
        self.id = id;
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Message {
    code: ControlCode,
    timestamp: i64,
    group: Group,
    sender: Member,
    msg: String,
}

impl Message {
    pub fn new(
        code: ControlCode,
        timestamp: i64,
        group: Group,
        sender: Member,
        msg: String,
    ) -> Message {
        Message {
            code,
            timestamp,
            group,
            sender,
            msg,
        }
    }
    pub fn new_default(code: ControlCode, group: Group, sender: Member, msg: String) -> Message {
        Message {
            code,
            timestamp: Utc::now().timestamp(),
            group,
            sender,
            msg,
        }
    }

    pub fn update_timestamp(&mut self) {
        self.timestamp = Utc::now().timestamp();
    }

    pub fn update_sender_address(&mut self, address: SocketAddr) {
        self.sender.set_address(address);
    }

    pub fn set_timestamp(&mut self, timestamp: i64) {
        self.timestamp = timestamp;
    }

    pub fn get_timestamp(&self) -> i64 {
        self.timestamp
    }

    pub fn set_code(&mut self, code: ControlCode) {
        self.code = code;
    }

    pub fn get_code(&self) -> &ControlCode {
        &self.code
    }

    pub fn set_message(&mut self, msg: String) {
        self.msg = msg;
    }

    pub fn get_message(&self) -> &String {
        &self.msg
    }

    pub fn get_sender(&self) -> &Member {
        &self.sender
    }

    pub fn get_group(&self) -> &Group {
        &self.group
    }
}

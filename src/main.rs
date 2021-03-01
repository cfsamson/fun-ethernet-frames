use std::{convert::TryInto, default, mem, vec};

const PREAMBLE_LEN: usize = 7;
const PREAMBLE: &[u8] = &[0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55];
const SFD: u8 = 0b10101011;

fn main() {
    let data: &[u8; 18] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 1, 2, 3, 4, 99, 99, 99, 99];
    let data2: &[u8; 14] = &[4, 5, 6, 7, 8, 9, 1, 2, 3, 4, 99, 99, 99, 99];
    let mut frames = vec![];

    let mut decoder = Decoder::new();

    for b in data {
        if let Some(frame) = decoder.recv_byte(*b) {
            frames.push(frame);
        }
    }

    for b in data2 {
        if let Some(frame) = decoder.recv_byte(*b) {
            frames.push(frame);
        }
    }

    for frame in frames {
        println!("{:#?}", frame);
    }
}

struct Decoder {
    pos: usize,
    interim: [u8; 8],
    payload_len: usize,
    frame: EthFrame,
    state: DecodeState,
}

impl Decoder {
    pub fn new() -> Self {
        Decoder {
            pos: 0,
            state: DecodeState::Waiting,
            interim: Default::default(),
            payload_len: 0,
            frame: EthFrame::default(),
        }
    }

    pub fn recv_byte(&mut self, byte: u8) -> Option<EthFrame> {
        use DecodeState::*;
        match self.state {
            Waiting => self.step_waiting(byte),
            Preamble => self.step_preamble(byte),
            Sfd => self.step_sfd(byte),
            RxMac => self.step_rx_mac(byte),
            TxMac => self.step_tx_mac(byte),
            Tag802 => self.step_tag802(byte),
            Payload => self.step_payload(byte),
            Checksum => self.step_checksum(byte),
            Finished => return self.step_finished(),
            Invalid => {
                // TODO: Handle invalid frame
                self.state = DecodeState::Waiting;
                self.clear_all();
                println!("Invalid frame");
            }
        }

        None
    }

    fn step_waiting(&mut self, b: u8) {
        self.state = DecodeState::Preamble;
        self.interim[self.pos] = b;
        self.pos += 1;
    }

    fn step_preamble(&mut self, b: u8) {
        if PREAMBLE[self.pos] != b {
            self.state = DecodeState::Waiting;
            self.clear_pos_and_interim();
            return;
        }

        self.pos += 1;
        if self.pos == PREAMBLE_LEN {
            self.clear_pos_and_interim();
            self.state = DecodeState::Sfd;
        }
    }

    fn step_sfd(&mut self, b: u8) {
        if b == SFD {
            self.state = DecodeState::RxMac;
        } else {
            self.state = DecodeState::Waiting;
        }
    }

    fn step_rx_mac(&mut self, b: u8) {
        self.frame.rx_mac[self.pos] = b;
        self.pos += 1;

        if self.pos == self.frame.rx_mac.len() {
            self.clear_pos_and_interim();
            self.state = DecodeState::TxMac;
        }
    }

    fn step_tx_mac(&mut self, b: u8) {
        self.frame.tx_mac[self.pos] = b;
        self.pos += 1;

        if self.pos == self.frame.tx_mac.len() {
            self.clear_pos_and_interim();
            self.state = DecodeState::Tag802;
        }
    }

    fn step_tag802(&mut self, b: u8) {
        let tag = match self.frame.tag802.as_mut() {
            None => return,
            Some(tag) => tag,
        };

        tag[self.pos] = b;
        self.pos += 1;

        if self.pos == tag.len() {
            let n = u16::from_be_bytes(*tag);
            self.payload_len = n as usize;
            self.state = DecodeState::Payload;
            self.clear_pos_and_interim();
        }
    }

    fn step_payload(&mut self, b: u8) {
        self.frame.payload.push(b);
        if self.frame.payload.len() == self.payload_len {
            self.state = DecodeState::Checksum;
        }
    }

    fn step_checksum(&mut self, b: u8) {
        self.interim[self.pos] = b;
        self.pos += 1;

        if self.pos == 4 {
            let crc_32 = self.hash_crc32();
            let crc: [u8; 4] = self.interim[0..4].try_into().unwrap();
            let verify = u32::from_be_bytes(crc);
            println!("crc_32: {}, got: {}", crc_32, verify);

            if crc_32 == verify {
                self.state = DecodeState::Finished;
                self.clear_pos_and_interim();
            } else {
                self.state = DecodeState::Invalid;
            }
        }
    }

    fn step_finished(&mut self) -> Option<EthFrame> {
        let frame = mem::replace(&mut self.frame, EthFrame::default());
        self.clear_all();
        self.state = DecodeState::Waiting;
        Some(frame)
    }

    fn hash_crc32(&self) -> u32 {
        let mut hasher = crc32fast::Hasher::new_with_initial(0xFFFFFFFF);
        hasher.update(&self.frame.rx_mac);
        hasher.update(&self.frame.tx_mac);
        hasher.update(&self.frame.tag802.unwrap());
        hasher.update(&self.frame.payload);
        hasher.finalize()
    }

    fn clear_pos_and_interim(&mut self) {
        self.pos = 0;
        self.interim = Default::default();
    }

    fn clear_all(&mut self) {
        self.clear_pos_and_interim();
        self.payload_len = 0;
    }
}

#[derive(Debug)]
struct EthFrame {
    rx_mac: [u8; 6],
    tx_mac: [u8; 6],
    tag802: Option<[u8; 2]>,
    payload: Vec<u8>,
}

impl Default for EthFrame {
    fn default() -> Self {
        Self {
            rx_mac: Default::default(),
            tx_mac: Default::default(),
            tag802: Some(Default::default()),
            payload: Vec::with_capacity(1500),
        }
    }
}

enum DecodeState {
    Waiting,
    Preamble,
    Sfd,
    RxMac,
    TxMac,
    Tag802,
    Payload,
    Checksum,
    Finished,
    Invalid,
}

// enum Decoder {
//     Waiting,
//     Preamble{
//         pos: usize,
//         preamble: [u8;8],
//     },
//     RxMac {
//         rx_mac: [u8; 6],
//     },
//     TxMac {
//         rx_mac: [u8; 6],
//         tx_mac: [u8; 6],
//     },
//     Tag802 {
//         rx_mac: [u8; 6],
//         tx_mac: [u8; 6],
//         tag802: [u8; 2],
//     },
//     Payload {
//         rx_mac: [u8; 6],
//         tx_mac: [u8; 6],
//         tag802: Option<[u8; 2]>,
//         payload: Vec<u8>,
//     },
//     Checksum {
//         rx_mac: [u8; 6],
//         tx_mac: [u8; 6],
//         tag802: Option<[u8; 2]>,
//         payload: Vec<u8>,
//         checksum: [u8; 8],
//     },
//     Finished {
//         rx_mac: [u8; 6],
//         tx_mac: [u8; 6],
//         tag802: Option<[u8; 2]>,
//         payload: Vec<u8>,
//     },

//     Invalid,
// }

// impl Decoder {
//     pub fn recv_byte(self, byte: u8) -> Self {
//         use Decoder::*;
//         match self {
//             Waiting => Preamble { preamble: [byte,0,0,0,0,0,0,0], pos: 0},
//             Preamble { mut preamble, mut pos } => {
//                 preamble[pos] = byte;
//                 pos += 1;

//                 if pos == preamble.len() {
//                     // Validate preamble
//                     return RxMac {
//                         rx_mac: [0,0,0,0,0,0]
//                     }
//                 }

//                 Preamble {preamble, pos }
//             }

//             _ => todo!()
//         }
//     }
// }

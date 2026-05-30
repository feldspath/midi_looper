mod session;
use self::session::*;

use std::fmt::Display;
use std::io::Write;

use log::{debug, info};
use mseq::{Instruction, MSeqError, MidiInParam, MidiMessage};
use thiserror::Error;

const INPUT_MIDI_CHANNEL: u8 = 10;

struct Looper {
    sessions: [Vec<Session>; 16],
    current_session: Session,
    record: bool,
    start_step: u32,
    current_midi_channel: u8,
    running: bool,
    stop_requested: bool,
    bpm: u8,
}

impl Looper {
    fn new() -> Self {
        Looper {
            sessions: Default::default(),
            current_session: Session::default(),
            record: false,
            start_step: 0,
            current_midi_channel: 1,
            running: true,
            stop_requested: false,
            bpm: 120,
        }
    }

    fn update_info(&self) {
        info!("{}", &self);
    }

    fn midi_channel_up(&mut self) {
        self.current_midi_channel = (self.current_midi_channel + 1).min(16);
    }
    fn midi_channel_down(&mut self) {
        self.current_midi_channel = (self.current_midi_channel - 1).max(1);
    }
    fn pop_last_session(&mut self) {
        let idx = (self.current_midi_channel - 1) as usize;
        if self.record {
            self.current_session.clear();
            self.record = false;
        } else if !self.sessions[idx].is_empty() {
            self.sessions[idx].pop();
        }
    }
    fn remove_first_session(&mut self) {
        let idx = (self.current_midi_channel - 1) as usize;
        if !self.sessions[idx].is_empty() {
            self.sessions[idx].remove(0);
        }
    }
    fn toggle_recording(&mut self, current_step: u32) {
        if self.record {
            // stop recording
            self.record = false;
            self.current_session.finalize(self.start_step, current_step);
            let idx = (self.current_midi_channel - 1) as usize;
            self.sessions[idx].push(std::mem::take(&mut self.current_session));
        } else {
            // start recording
            self.record = true;
            self.start_step = current_step;
        }
    }
}

impl Display for Looper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.record {
            write!(
                f,
                "current midi channel: {} -- recording...",
                self.current_midi_channel
            )
        } else {
            write!(
                f,
                "[{}] current midi channel: {}",
                self.bpm, self.current_midi_channel
            )
        }
    }
}

impl mseq::Conductor for Looper {
    fn init(&mut self, context: &mut mseq::Context) -> Vec<mseq::Instruction> {
        context.start();
        vec![]
    }

    fn update(&mut self, context: &mut mseq::Context) -> Vec<mseq::Instruction> {
        if !self.running {
            context.quit();
        }
        context.set_bpm(self.bpm);

        let step = context.get_step();

        if step % (24 * 4) == 0 {
            self.stop_requested = false;
        }

        let mut instructions = vec![];

        // play all instructions that play this step
        instructions.extend(
            self.sessions
                .iter()
                .flat_map(|midi_session| midi_session.iter())
                .flat_map(|sess| sess.instructions_this_step(step)),
        );

        instructions
    }

    fn handle_input(&mut self, input: MidiMessage, ctx: &mseq::Context) -> Vec<Instruction> {
        let step = ctx.get_step();
        debug!("midi message: {:?}", input);

        match input {
            MidiMessage::NoteOff { channel: _, note } => {
                if self.record {
                    let inst = Instruction::StopNote {
                        midi_note: note,
                        channel_id: self.current_midi_channel,
                    };
                    self.current_session.record_instruction(inst, step);
                }
            }
            MidiMessage::NoteOn { channel: _, note } => {
                if self.record {
                    let inst = Instruction::StartNote {
                        midi_note: note,
                        channel_id: self.current_midi_channel,
                    };
                    self.current_session.record_instruction(inst, step);
                }
            }
            MidiMessage::CC {
                channel,
                controller,
                value,
            } => {
                if value > 0 {
                    // pop
                    if channel == INPUT_MIDI_CHANNEL && controller == 16 {
                        debug!("pop last session");
                        self.pop_last_session();
                    }

                    // remove first
                    if channel == INPUT_MIDI_CHANNEL && controller == 20 {
                        debug!("remove first session");
                        self.remove_first_session();
                    }

                    // recording
                    if channel == INPUT_MIDI_CHANNEL && controller == 17 {
                        debug!("toggle recording");
                        self.toggle_recording(step);
                    }

                    // midi channel down
                    if !self.record && channel == INPUT_MIDI_CHANNEL && controller == 19 {
                        debug!("midi channel down");
                        self.midi_channel_down();
                    }
                    // midi channel up
                    if !self.record && channel == INPUT_MIDI_CHANNEL && controller == 23 {
                        debug!("midi channel up");
                        self.midi_channel_up();
                    }

                    // stop program
                    if channel == INPUT_MIDI_CHANNEL && controller == 22 {
                        debug!("stop program");
                        if self.stop_requested {
                            self.running = false;
                        } else {
                            self.stop_requested = true;
                        }
                    }

                    // set bpm
                    if channel == 1 && controller == 70 {
                        self.bpm = 50 + value;
                    }

                    self.update_info();
                }
            }
            _ => {}
        }

        let output = set_note_message_channel(input, self.current_midi_channel);

        // passthrough all midi messages
        vec![Instruction::MidiMessage {
            midi_message: output,
        }]
    }
}

#[derive(Error, Debug)]
pub enum MainError {
    #[error("MSeq Error: {0}")]
    MSeq(#[from] MSeqError),
}

fn main() -> Result<(), MainError> {
    env_logger::Builder::from_default_env()
        .format(|buf, record| {
            let level_style = buf.default_level_style(record.level());

            writeln!(
                buf,
                "[{level_style}{}{level_style:#}] {}",
                record.level(),
                record.args()
            )
        })
        .init();

    let looper = Looper::new();
    let midi_in_params = MidiInParam {
        ignore: mseq::Ignore::None,
        port: None,
        slave: false,
    };

    mseq::run(looper, None, Some(midi_in_params))?;

    Ok(())
}

fn set_note_message_channel(message: MidiMessage, channel: u8) -> MidiMessage {
    match message {
        MidiMessage::NoteOff { channel: _, note } => MidiMessage::NoteOff { channel, note },
        MidiMessage::NoteOn { channel: _, note } => MidiMessage::NoteOn { channel, note },
        MidiMessage::CC {
            channel: _,
            controller: _,
            value: _,
        } => message,
        MidiMessage::PC {
            channel: _,
            value: _,
        } => message,
        MidiMessage::Clock => message,
        MidiMessage::Start => message,
        MidiMessage::Continue => message,
        MidiMessage::Stop => message,
    }
}

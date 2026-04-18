use mseq::{Instruction, MSeqError, MidiInParam, MidiMessage, MidiNote};
use thiserror::Error;

// todo: stop all notes still playing
// maybe live note analysis is worth it after all

#[derive(Default, Debug)]
struct Session {
    instructions: Vec<(u32, Instruction)>,
    length_in_bars: u32,
    finalized: bool,
}

const BAR_LENGTH: u32 = 24 * 4;

impl Session {
    fn instructions_this_step(&self, step: u32) -> impl Iterator<Item = Instruction> {
        assert!(self.finalized);
        self.instructions
            .iter()
            .filter_map(move |(s, inst)| {
                if step % (BAR_LENGTH * self.length_in_bars) == *s {
                    Some(inst)
                } else {
                    None
                }
            })
            .cloned()
    }

    fn record_instruction(&mut self, instruction: Instruction, step: u32) {
        assert!(!self.finalized);
        match instruction {
            Instruction::PlayNote {
                midi_note,
                len: _,
                channel_id,
            } => self.instructions.push((
                step - 1,
                Instruction::StopNote {
                    midi_note,
                    channel_id,
                },
            )),
            Instruction::StartNote {
                midi_note,
                channel_id,
            } => self.instructions.push((
                step - 1,
                Instruction::StopNote {
                    midi_note,
                    channel_id,
                },
            )),
            _ => {}
        }
        self.instructions.push((step, instruction));
    }

    fn finalize(&mut self, start_step: u32, end_step: u32) {
        let start_bar_num = (start_step as f32 / BAR_LENGTH as f32).round() as u32;
        self.length_in_bars =
            ((end_step as f32 / BAR_LENGTH as f32).round() as u32 - start_bar_num).max(1);
        println!(
            "start bar: {}, length: {}",
            start_bar_num, self.length_in_bars
        );
        self.instructions.iter_mut().for_each(|(s, _)| {
            let t = *s as i32 - (start_bar_num * BAR_LENGTH) as i32;
            *s = (t + if t < 0 {
                (start_bar_num * BAR_LENGTH) as i32
            } else {
                0
            }) as u32;
        });
        self.finalized = true;
    }

    fn is_empty(&self) -> bool {
        self.instructions.is_empty()
    }

    fn clear(&mut self) {
        self.instructions.clear();
        self.finalized = false;
        self.length_in_bars = 0;
    }
}

struct Looper {
    sessions: Vec<Session>,
    current_session: Session,
    record: bool,
    start_step: u32,
}

impl Looper {
    fn new() -> Self {
        Looper {
            sessions: vec![],
            current_session: Session::default(),
            record: false,
            start_step: 0,
        }
    }
}

impl mseq::Conductor for Looper {
    fn init(&mut self, context: &mut mseq::Context) -> Vec<mseq::Instruction> {
        context.start();
        vec![]
    }

    fn update(&mut self, context: &mut mseq::Context) -> Vec<mseq::Instruction> {
        let step = context.get_step();
        let mut instructions = vec![];

        // play rythm
        // if step % 24 == 0 {
        //     let vel = if (step / 24) % 4 == 0 { 127 } else { 90 };
        //     instructions.push(Instruction::PlayNote {
        //         midi_note: MidiNote {
        //             note: mseq::Note::C,
        //             octave: 3,
        //             vel,
        //         },
        //         len: 6,
        //         channel_id: 2,
        //     });
        // }

        // play all instructions that play this step
        instructions.extend(
            self.sessions
                .iter()
                .map(|sess| sess.instructions_this_step(step))
                .flatten(),
        );

        instructions
    }

    fn handle_input(&mut self, input: MidiMessage, ctx: &mseq::Context) -> Vec<Instruction> {
        let step = ctx.get_step();
        println!("midi message: {:?}", input);
        match input {
            MidiMessage::NoteOff { channel, note } => {
                if self.record {
                    let inst = Instruction::StopNote {
                        midi_note: note,
                        channel_id: channel,
                    };
                    self.current_session.record_instruction(inst, step);
                }
            }
            MidiMessage::NoteOn { channel, note } => {
                if self.record {
                    let inst = Instruction::StartNote {
                        midi_note: note,
                        channel_id: channel,
                    };
                    self.current_session.record_instruction(inst, step);
                }
            }
            MidiMessage::CC {
                channel,
                controller,
                value,
            } => {
                // pop
                if channel == 10 && controller == 16 && value > 0 {
                    if self.current_session.is_empty() {
                        self.sessions.pop();
                    } else {
                        self.current_session.clear();
                    }
                }

                // remove first
                if channel == 10 && controller == 20 && value > 0 {
                    if self.sessions.len() > 0 {
                        self.sessions.remove(0);
                    }
                }

                // start recording
                if channel == 10 && controller == 17 && value > 0 {
                    println!("start recording");
                    self.record = true;
                    self.start_step = step;
                }
                // stop recording
                if channel == 10 && controller == 18 && value > 0 {
                    println!("stop recording");
                    self.record = false;
                    self.current_session.finalize(self.start_step, step);
                    self.sessions
                        .push(std::mem::take(&mut self.current_session));
                }
            }
            _ => {}
        }

        // passthrough all midi messages
        vec![Instruction::MidiMessage {
            midi_message: input,
        }]
    }
}

#[derive(Error, Debug)]
pub enum MainError {
    #[error("MSeq Error: {0}")]
    MSeq(#[from] MSeqError),
}

fn main() -> Result<(), MainError> {
    let looper = Looper::new();
    let midi_in_params = MidiInParam {
        ignore: mseq::Ignore::None,
        port: None,
        slave: false,
    };

    mseq::run(looper, None, Some(midi_in_params))?;

    Ok(())
}

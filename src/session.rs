use log::info;
use mseq::Instruction;

const BAR_LENGTH: u32 = 24 * 4;

#[derive(Default, Debug)]
pub struct Session {
    instructions: Vec<(u32, Instruction)>,
    length_in_bars: u32,
    finalized: bool,
}

impl Session {
    pub fn instructions_this_step(&self, step: u32) -> impl Iterator<Item = Instruction> {
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

    pub fn record_instruction(&mut self, instruction: Instruction, step: u32) {
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

    pub fn finalize(&mut self, start_step: u32, end_step: u32) {
        let start_bar_num = (start_step as f32 / BAR_LENGTH as f32).round() as u32;
        self.length_in_bars =
            ((end_step as f32 / BAR_LENGTH as f32).round() as u32 - start_bar_num).max(1);
        info!(
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

    pub fn clear(&mut self) {
        self.instructions.clear();
        self.finalized = false;
        self.length_in_bars = 0;
    }
}

use crate::{BitReader, ByteReader, Color, Transitions};
use crate::maps::{Mode, black, white, mode, EDFB_HALF, EOL};


fn with_markup<D, R>(decoder: D, reader: &mut R) -> Option<u16>
    where D: Fn(&mut R) -> Option<u16>
{
    let mut sum = 0;
    while let Some(n) = decoder(reader) {
        //print!("{} ", n);
        sum += n;
        if n < 64 {
            //println!("= {}", sum);
            return Some(sum)
        }
    }
    None
}

fn colored(current: Color, reader: &mut impl BitReader) -> Option<u16> {
    //println!("{:?}", current);
    match current {
        Color::Black => with_markup(black::decode, reader),
        Color::White => with_markup(white::decode, reader),
    }
}

/// Turn a list of color changing position into an iterator of pixel colors
///
/// The width of the line/image has to be given in `width`.
/// The iterator will produce exactly that many items.
pub fn pels(line: &[u16], width: u16) -> impl Iterator<Item=Color> + '_ {
    use std::iter::repeat;
    let mut color = Color::White;
    let mut last = 0;
    let pad_color = if line.len() & 1 == 1 {
        !color
    } else { 
        color
    };
    line.iter().flat_map(move |&p| {
        let c = color;
        color = !color;
        let n = p.saturating_sub(last);
        last = p;
        repeat(c).take(n as usize)
    }).chain(repeat(pad_color)).take(width as usize)
}

/// Decode a Group 3 encoded image.
/// 
/// The callback `line_cb` is called for each decoded line.
/// The argument is the list of positions of color change, starting with white.
/// 
/// To obtain an iterator over the pixel colors, the `pels` function is provided.
pub fn decode_g3(input: impl Iterator<Item=u8>, mut line_cb: impl FnMut(&[u16])) -> Option<()> {
    let mut reader = ByteReader::new(input);
    let mut current = vec![];
    reader.expect(EOL).unwrap();
    
    'a: loop {
        let mut a0 = 0;
        let mut color = Color::White;
        while let Some(p) = colored(color, &mut reader) {
            a0 += p;
            current.push(a0);
            color = !color;
        }
        reader.expect(EOL).unwrap();
        line_cb(&current);
        current.clear();

        for _ in 0 .. 6 {
            if reader.peek(EOL.len) == Some(EOL.data) {
                reader.consume(EOL.len);
            } else {
                continue 'a;
            }
        }
        break;
    }
    Some(())
}

/// Decode a Group 4 Image
/// 
/// - `width` is the width of the image.
/// - The callback `line_cb` is called for each decoded line.
///   The argument is the list of positions of color change, starting with white.
/// 
///   If `height` is specified, at most that many lines will be decoded,
///   otherwise data is decoded until the end-of-block marker (or end of data).
/// 
/// To obtain an iterator over the pixel colors, the `pels` function is provided.
pub fn decode_g4(input: impl Iterator<Item=u8>, width: u16, height: Option<u16>, mut line_cb: impl FnMut(&[u16])) -> Option<()> {
    let mut reader = ByteReader::new(input);
    let mut reference: Vec<u16> = vec![];
    let mut current: Vec<u16> = vec![];

    let limit = height.unwrap_or(u16::MAX);
    'outer: for y in 0 .. limit {
        let mut transitions = Transitions::new(&reference);
        let mut a0 = 0;
        let mut color = Color::White;
        let mut start_of_row = true;
        //println!("\n\nline {}", y);
        
        loop {
            //reader.print_peek();
            let mode = match mode::decode(&mut reader) {
                Some(mode) => mode,
                None => break 'outer,
            };
            //println!("  {:?}, color={:?}, a0={}", mode, color, a0);
            
            match mode {
                Mode::Pass => {
                    if start_of_row && color == Color::White {
                        transitions.pos += 1;
                    } else {
                        transitions.next_color(a0, !color, false)?;
                    }
                    //println!("b1={}", b1);
                    if let Some(b2) = transitions.next() {
                        //println!("b2={}", b2);
                        a0 = b2;
                    }
                }
                Mode::Vertical(delta) => {
                    let b1 = transitions.next_color(a0, !color, start_of_row).unwrap_or(width);
                    let a1 = (b1 as i16 + delta as i16) as u16;
                    if a1 >= width {
                        break;
                    }
                    //println!("transition to {:?} at {}", !color, a1);
                    current.push(a1);
                    color = !color;
                    a0 = a1;
                    if delta < 0 {
                        transitions.seek_back(a0);
                    }
                }
                Mode::Horizontal => {
                    let a0a1 = colored(color, &mut reader)?;
                    let a1a2 = colored(!color, &mut reader)?;
                    let a1 = a0 + a0a1;
                    let a2 = a1 + a1a2;
                    //println!("a0a1={}, a1a2={}, a1={}, a2={}", a0a1, a1a2, a1, a2);
                    
                    current.push(a1);
                    if a2 >= width {
                        break;
                    }
                    current.push(a2);
                    a0 = a2;
                }
                Mode::Extension => {
                    let xxx = reader.peek(3)?;
                    println!("extension: {:03b}", xxx);
                    reader.consume(3);
                    println!("{:?}", current);
                    break 'outer;
                }
                Mode::EOF => break 'outer,
            }
            start_of_row = false;

            if a0 >= width {
                break;
            }
        }
        //println!("{:?}", current);

        line_cb(&current);
        std::mem::swap(&mut reference, &mut current);
        current.clear();
    }
    if height.is_none() {
        reader.expect(EDFB_HALF).ok()?;
    }
    reader.print_remaining();

    Some(())
}

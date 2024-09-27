use std::io::{IsTerminal, Write};
use std::{io, thread};

use crate::element::Size;
use crate::{display, Constraint, Display, Element, Line, Paint};

use crossbeam_channel as chan;
use radicle_signals as signals;
use termion::event::{Event, Key, MouseButton, MouseEvent};
use termion::{input::TermRead, raw::IntoRawMode, screen::IntoAlternateScreen};

/// How many lines to scroll when the mouse wheel is used.
const MOUSE_SCROLL_LINES: usize = 3;

/// Pager error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Channel(#[from] chan::RecvError),
}

/// A pager for the given element. Re-renders the element when the terminal is resized so that
/// it doesn't wrap. If the output device is not a TTY, just prints the element via
/// [`Element::print`].
///
/// # Signal Handling
///
/// This will install handlers for the pager until finished by the user, with there
/// being only one element handling signals at a time. If the pager cannot install
/// handlers, then it will return with an error.
pub fn page<E: Element + Send + 'static>(element: E) -> Result<(), Error> {
    let (events_tx, events_rx) = chan::unbounded();
    let (signals_tx, signals_rx) = chan::unbounded();

    signals::install(signals_tx)?;

    thread::spawn(move || {
        for e in io::stdin().events() {
            events_tx.send(e).ok();
        }
    });
    let result = thread::spawn(move || main(element, signals_rx, events_rx))
        .join()
        .unwrap();

    signals::uninstall()?;

    result
}

fn main<E: Element>(
    element: E,
    signals_rx: chan::Receiver<signals::Signal>,
    events_rx: chan::Receiver<Result<Event, io::Error>>,
) -> Result<(), Error> {
    let stdout = io::stdout();
    if !stdout.is_terminal() {
        element.print();
        return Ok(());
    }
    let raw = stdout.into_raw_mode()?;
    let mut stdout = termion::input::MouseTerminal::from(raw).into_alternate_screen()?;
    let (mut width, mut height) = termion::terminal_size()?;
    let mut lines = element.render(Constraint::max(Size::new(width as usize, height as usize)));
    let mut line = 0;

    render(&mut stdout, lines.as_slice(), line, (width, height))?;

    loop {
        chan::select! {
            recv(signals_rx) -> signal => {
                match signal? {
                    signals::Signal::WindowChanged => {
                        let (w, h) = termion::terminal_size()?;

                        lines = element.render(Constraint::max(Size::new(w as usize, h as usize)));
                        width = w;
                        height = h;
                    }
                    signals::Signal::Interrupt | signals::Signal::Terminate => {
                        break;
                    }
                    _ => continue,
                }
            }
            recv(events_rx) -> event => {
                let event = event??;
                let page = height as usize - 1; // Don't count the status bar.
                let end = if page > lines.len() { 0 } else { lines.len() - page };
                let prev = line;

                match event {
                    Event::Key(key) => match key {
                        Key::Up | Key::Char('k') => {
                            line = line.saturating_sub(1);
                        }
                        Key::Home => {
                            line = 0;
                        }
                        Key::End | Key::Char('G') => {
                            line = end;
                        }
                        Key::PageUp | Key::Char('b') => {
                            line = line.saturating_sub(page);
                        }
                        Key::PageDown | Key::Char(' ') => {
                            line = (line + page).min(end);
                        }
                        Key::Down | Key::Char('j') => {
                            if line < end {
                                line += 1;
                            }
                        }
                        Key::Char('q') => break,

                        _ => continue,
                    }
                    Event::Mouse(MouseEvent::Press(MouseButton::WheelDown, _, _)) => {
                        if line < end {
                            line += MOUSE_SCROLL_LINES;
                        }
                    }
                    Event::Mouse(MouseEvent::Press(MouseButton::WheelUp, _, _)) => {
                        line = line.saturating_sub(MOUSE_SCROLL_LINES);
                    }
                    _ => continue,
                }
                // Don't re-render if there's no change in line.
                if line == prev {
                    continue;
                }
            }
        }
        render(&mut stdout, &lines, line, (width, height))?;
    }
    Ok(())
}

fn render<W: Write>(
    out: &mut W,
    lines: &[Line],
    start_line: usize,
    (width, height): (u16, u16),
) -> io::Result<()> {
    write!(
        out,
        "{}{}",
        termion::clear::All,
        termion::cursor::Goto(1, 1)
    )?;

    let content_length = lines.len();
    let window_size = height as usize - 1;
    let end_line = if start_line + window_size > content_length {
        content_length
    } else {
        start_line + window_size
    };
    // Render content.
    for (ix, line) in lines[start_line..end_line].iter().enumerate() {
        write!(out, "{}{}", termion::cursor::Goto(1, ix as u16 + 1), display(line))?;
    }
    // Render progress meter.
    write!(
        out,
        "{}{}",
        termion::cursor::Goto(width - 3, height),
        display(&Paint::new(format!(
            "{:.0}%",
            end_line as f64 / lines.len() as f64 * 100.
        ))
        .dim())
    )?;
    // Render cursor input area.
    write!(
        out,
        "{}{}",
        termion::cursor::Goto(1, height),
        display(&Paint::new(":").dim())
    )?;
    out.flush()?;

    Ok(())
}

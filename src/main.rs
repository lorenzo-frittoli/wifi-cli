extern crate termion;

use std::io::{self, stdin, stdout, Stdout, Write};
use std::process::Command;
use std::u8;
use termion::event::{Event, Key};
use termion::input::TermRead;
use termion::raw::{IntoRawMode, RawTerminal};

enum Response {
    Continue,
    Select,
    Quit,
}

#[derive(Debug)]
enum AuthError {
    Program(Box<dyn std::error::Error>),
    Command(String),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::Program(inner) => {
                write!(f,
                       "The program encountered the following generic error of type Box<dyn std::error::Error>:\n{}",
                       inner)
            }
            AuthError::Command(inner) => {
                write!(f,
                       "The output authentication command (type std::process::Output) threw the following error on execution:\n{}",
                       inner)
            }
        }
    }
}

impl std::error::Error for AuthError {}

fn main() -> io::Result<()> {
    let stdin = stdin();
    let mut stdout = stdout().into_raw_mode()?;

    let mut selector_pos: u8 = 0;
    let mut wifi_list: String = refresh(&mut stdout, &selector_pos)?;

    let mut state = 0;
    let mut pwd = String::new();
    let mut ssid = String::new();

    let mut event_iter = stdin.events();
    loop {
        match state {
            0 => {
                if let Some(Ok(evt)) = event_iter.next() {
                    let response = match_evt(evt, &mut stdout, &wifi_list, &mut selector_pos)?;
                    match response {
                        Response::Continue => wifi_list = refresh(&mut stdout, &selector_pos)?,
                        Response::Select => {
                            state = 1;
                            ssid = get_ssids(&wifi_list)[selector_pos as usize].clone();
                        }
                        Response::Quit => break,
                    }
                }
            }
            1 => {
                clear_and_write(
                    &mut stdout,
                    &format!("CONNECTING\nSSID: {}\nPassword: ", &ssid),
                )?;
                RawTerminal::suspend_raw_mode(&stdout)?;
                if let Some(Ok(evt)) = event_iter.next() {
                    match evt {
                        Event::Key(Key::Esc) => break,
                        Event::Key(Key::Char(c)) => match c {
                            '\n' => state = 2,
                            _ => pwd.push(c),
                        },
                        _ => {}
                    }
                }
            }
            2 => {
                match authenticate(&ssid, &pwd) {
                    Ok(msg) => {
                        clear_and_write(
                            &mut stdout,
                            &format!("You are connected to {}\nCommand Output:\n{}", &ssid, msg),
                        )?;
                    }
                    Err(err) => clear_and_write(&mut stdout, &format!("{}", err))?,
                }
                break;
            }
            _ => break,
        }

        stdout.flush()?;
    }

    Ok(())
}

fn match_evt(
    evt: Event,
    stdout: &mut RawTerminal<Stdout>,
    wifi_list: &str,
    selector_pos: &mut u8,
) -> io::Result<Response> {
    return match evt {
        // Arrows
        Event::Key(Key::Up) => {
            *selector_pos = if *selector_pos > 0 {
                *selector_pos - 1
            } else {
                0
            };
            Ok(Response::Continue)
        }
        Event::Key(Key::Down) => {
            let bound: u8 = wifi_list.lines().skip(1).fold(0, |acc, _| acc + 1) - 1 as u8;
            *selector_pos = (*selector_pos + 1).clamp(0, bound);
            Ok(Response::Continue)
        }

        Event::Key(Key::Char('r')) => Ok(Response::Continue),
        Event::Key(Key::Char('q')) => Ok(Response::Quit),
        Event::Key(Key::Char('\n')) => Ok(Response::Select),

        _ => Ok(Response::Continue),
    };
}

fn wifi_list() -> io::Result<String> {
    let output = Command::new("nmcli")
        .arg("device")
        .arg("wifi")
        .arg("list")
        .output()?;

    return Ok(String::from_utf8(output.stdout).unwrap());
}

fn get_ssids(wifi_list: &str) -> Vec<String> {
    return wifi_list
        .lines()
        .skip(1)
        .map(|l| {
            l[8..]
                .split(' ')
                .next()
                .expect("Failed to parse ssid")
                .to_string()
        })
        .collect();
}

fn refresh(stdout: &mut RawTerminal<Stdout>, selector_pos: &u8) -> io::Result<String> {
    let wifi_list = wifi_list()?;
    clear_and_write(stdout, &wifi_list)?;
    write!(
        stdout,
        "{}{}{}",
        ' ',
        termion::cursor::Goto(1, *selector_pos as u16 + 2),
        '>',
    )?;

    return Ok(wifi_list);
}

fn clear_and_write(stdout: &mut RawTerminal<Stdout>, contents: &str) -> io::Result<()> {
    // Suspend raw mode to write nicely
    RawTerminal::suspend_raw_mode(&stdout)?;
    write!(
        stdout,
        "{}{}{}",
        termion::clear::All,
        termion::cursor::Goto(1, 1),
        contents,
    )?;
    stdout.flush()?;

    // Re-enable raw mode for better input
    RawTerminal::activate_raw_mode(&stdout)?;

    Ok(())
}

fn authenticate(ssid: &str, pwd: &str) -> Result<String, AuthError> {
    // Authenticate
    let output = Command::new("nmcli")
        .args(["device", "wifi", "connect", ssid, "password", pwd])
        .output()
        .map_err(|e| AuthError::Program(Box::new(e)))?;

    return if output.status.success() {
        Ok(String::from_utf8(output.stdout).unwrap())
    } else {
        Err(AuthError::Command(
            String::from_utf8(output.stderr).unwrap(),
        ))
    };
}

//! Technical words as they are written: "uart" → "UART", "blue tooth" → "Bluetooth", "3.3 volts"
//! → "3.3 V", "32 bit timer" → "32-bit timer". Dictation only, after the user's own Dictionary
//! (whose entries win), and never in code editors (the "exact" style).
//!
//! Only words with no everyday meaning are here: "rust", "can", "led" and "ram" are ordinary
//! words too, so they are left for the user's Dictionary.

use super::glossary::replace;
use super::text::{token_key, tokens, trailing_punctuation};

/// Spoken (any case, whole words) → written.
const TERMS: [(&str, &str); 51] = [
    ("kay cad", "KiCad"), ("key cad", "KiCad"), ("kai cad", "KiCad"), ("via score", "VS Code"), ("vs code", "VS Code"),
    ("bluetooth low energy", "Bluetooth Low Energy"), ("hard fault", "HardFault"),
    ("uart", "UART"), ("spi", "SPI"), ("i2c", "I²C"), ("i 2 c", "I²C"), ("i2s", "I²S"), ("gpio", "GPIO"),
    ("adc", "ADC"), ("dac", "DAC"), ("pwm", "PWM"), ("mqtt", "MQTT"), ("m q t t", "MQTT"), ("tls", "TLS"),
    ("dma", "DMA"), ("ota", "OTA"), ("cpu", "CPU"), ("gpu", "GPU"), ("usb", "USB"), ("pcb", "PCB"), ("ble", "BLE"),
    ("rtos", "RTOS"), ("mosfet", "MOSFET"), ("jtag", "JTAG"), ("esp32", "ESP32"), ("esp 32", "ESP32"),
    ("wifi", "Wi-Fi"), ("wi fi", "Wi-Fi"), ("bluetooth", "Bluetooth"), ("blue tooth", "Bluetooth"),
    ("sqlite", "SQLite"), ("sequel lite", "SQLite"), ("sql lite", "SQLite"), ("c sharp", "C#"),
    ("j link", "J-Link"), ("jlink", "J-Link"), ("board rate", "baud rate"), ("tauri", "Tauri"),
    ("pull up resistor", "pull-up resistor"), ("pull down resistor", "pull-down resistor"),
    ("real time operating system", "real-time operating system"), ("fail safe", "fail-safe"),
    ("boot loader", "bootloader"), ("usb type c", "USB Type-C"), ("type c", "Type-C"), ("lora", "LoRa"),
];

/// Units after a number: the spoken words → the symbol.
const UNITS: [(&[&str], &str); 31] = [
    (&["volts"], "V"), (&["volt"], "V"), (&["milliamps"], "mA"), (&["milliamp"], "mA"), (&["milli", "amps"], "mA"),
    (&["microamps"], "µA"), (&["micro", "amps"], "µA"), (&["kilohertz"], "kHz"), (&["kilo", "hertz"], "kHz"),
    (&["megahertz"], "MHz"), (&["mega", "hertz"], "MHz"), (&["hertz"], "Hz"), (&["milliseconds"], "ms"),
    (&["millisecond"], "ms"), (&["kilobytes"], "KB"), (&["kilo", "bytes"], "KB"), (&["megabytes"], "MB"),
    (&["gigabytes"], "GB"), (&["kilo", "ohms"], "kΩ"), (&["kilo", "ohm"], "kΩ"), (&["kiloohms"], "kΩ"),
    (&["ohms"], "Ω"), (&["milli", "farad"], "mF"), (&["millifarad"], "mF"), (&["micro", "farad"], "µF"),
    (&["microfarad"], "µF"), (&["kilo", "bits", "per", "second"], "kbit/s"), (&["kilobits", "per", "second"], "kbit/s"),
    (&["degrees", "celsius"], "°C"), (&["degree", "celsius"], "°C"), (&["milli", "volts"], "mV"),
];

/// Joined to the number before them when a noun follows: "32 bit timer" → "32-bit timer".
const COMPOUND_UNITS: [&str; 4] = ["bit", "layer", "pin", "channel"];

/// The windows versions people name: "windows 11" → "Windows 11". "the windows" stays.
const WINDOWS_VERSIONS: [&str; 4] = ["7", "8", "10", "11"];

/// Technical words, units and compounds as they are written.
pub fn apply(text: &str) -> String {
    let pairs: Vec<(String, String)> = TERMS.iter().map(|(s, w)| ((*s).to_string(), (*w).to_string())).collect();
    let text = replace(text, &pairs);
    let toks = tokens(&text);
    if toks.is_empty() {
        return text;
    }
    let mut out: Vec<String> = Vec::with_capacity(toks.len());
    let mut i = 0;
    while i < toks.len() {
        let token = &toks[i];
        let key = token_key(token);
        let number = is_number(token);
        let next = toks.get(i + 1).map(|t| token_key(t)).unwrap_or_default();
        // A punctuation mark after the number means the unit is not its own.
        let open = trailing_punctuation(token).is_empty();
        if number && open {
            if let Some((words, symbol)) = UNITS.iter().find(|(w, _)| unit_at(&toks, i + 1, w)) {
                let closing = trailing_punctuation(&toks[i + words.len()]);
                let sep = if symbol.starts_with('°') { "" } else { " " };
                out.push(format!("{token}{sep}{symbol}{closing}"));
                i += 1 + words.len();
                continue;
            }
            let noun_after = toks.get(i + 2).is_some_and(|t| t.chars().next().is_some_and(char::is_alphabetic));
            if COMPOUND_UNITS.contains(&next.as_str()) && trailing_punctuation(&toks[i + 1]).is_empty() && noun_after {
                out.push(format!("{token}-{}", toks[i + 1]));
                i += 2;
                continue;
            }
        }
        // "nrf 52840" / "nrf52840" → "nRF52840".
        if key == "nrf" && open && toks.get(i + 1).is_some_and(|t| is_number(t)) {
            out.push(format!("nRF{}", toks[i + 1]));
            i += 2;
            continue;
        }
        if let Some(digits) = key.strip_prefix("nrf").filter(|d| !d.is_empty() && d.chars().all(|c| c.is_ascii_digit())) {
            out.push(format!("nRF{digits}{}", trailing_punctuation(token)));
            i += 1;
            continue;
        }
        if key == "windows" && open && toks.get(i + 1).is_some_and(|t| WINDOWS_VERSIONS.contains(&token_key(t).as_str())) {
            out.push("Windows".into());
            i += 1;
            continue;
        }
        out.push(token.clone());
        i += 1;
    }
    out.join(" ")
}

/// Digits, maybe with a decimal point or thousands commas, or a number word up to ten.
fn is_number(token: &str) -> bool {
    let key = token.trim_end_matches(|c: char| !c.is_alphanumeric());
    let digits = !key.is_empty() && key.chars().all(|c| c.is_ascii_digit() || c == '.' || c == ',') && key.chars().next().is_some_and(|c| c.is_ascii_digit());
    digits || super::numbers::is_number_word(&token_key(token))
}

fn unit_at(toks: &[String], at: usize, words: &[&str]) -> bool {
    at + words.len() <= toks.len()
        && words.iter().enumerate().all(|(k, w)| token_key(&toks[at + k]) == *w)
        // Punctuation may close only the last word of the unit.
        && toks[at..at + words.len() - 1].iter().all(|t| trailing_punctuation(t).is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acronyms_and_names_are_written_as_they_should_be() {
        for (raw, expected) in [
            ("Configure the uart at 115200 board rate.", "Configure the UART at 115200 baud rate."),
            ("The data should be stored in sequel lite.", "The data should be stored in SQLite."),
            ("The backend is written in c sharp.", "The backend is written in C#."),
            ("The nrf 52840 should sleep.", "The nRF52840 should sleep."),
            ("Use j link for programming.", "Use J-Link for programming."),
            ("It runs on windows 10 and windows 11.", "It runs on Windows 10 and Windows 11."),
            ("Open the windows please.", "Open the windows please."),
            ("Use a 10 kΩ pull up resistor.", "Use a 10 kΩ pull-up resistor."),
            ("Use spi instead of i2c.", "Use SPI instead of I²C."),
        ] {
            assert_eq!(apply(raw), expected, "{raw}");
        }
    }

    #[test]
    fn misheard_tool_names_are_fixed_on_whole_words_only() {
        for (raw, expected) in [
            ("I used kay cad and via score.", "I used KiCad and VS Code."),
            ("Open key cad.", "Open KiCad."),
            ("Open kai cad.", "Open KiCad."),
            ("Open vs code.", "Open VS Code."),
            ("The discad and vias coreboard stay.", "The discad and vias coreboard stay."),
            ("Use spi and uart.", "Use SPI and UART."),
        ] {
            assert_eq!(apply(raw), expected, "{raw}");
        }
    }

    #[test]
    fn units_after_a_number_become_symbols() {
        for (raw, expected) in [
            ("It operates at 3.3 volts.", "It operates at 3.3 V."),
            ("From 10 milliamps to 2 milliamps.", "From 10 mA to 2 mA."),
            ("Sample at 1 kilo hertz.", "Sample at 1 kHz."),
            ("Use a 10 kilo ohm resistor.", "Use a 10 kΩ resistor."),
            ("Less than 100 milliseconds.", "Less than 100 ms."),
            ("Configure it at 500 kilo bits per second.", "Configure it at 500 kbit/s."),
            ("Up to 85 degrees Celsius.", "Up to 85°C."),
            ("Use a 32 bit timer.", "Use a 32-bit timer."),
            ("Use a four layer PCB.", "Use a four-layer PCB."),
        ] {
            assert_eq!(apply(raw), expected, "{raw}");
        }
        // No number before it: the word stays.
        assert_eq!(apply("The volts were low."), "The volts were low.");
        assert_eq!(apply("It has 32 bit."), "It has 32 bit.");
    }
}

// Claude-generated.
use quick_xml::Reader;
use quick_xml::events::Event;

#[derive(Default, Debug)]
pub struct Error {
    pub unique: String,
    pub tid: u32,
    pub kind: String,
    pub what: Option<String>,
    pub xwhat: Option<XWhat>,
    pub auxwhat: Vec<String>,
    pub xauxwhat: Option<XWhat>,
    pub stacks: Vec<Stack>,
}

#[derive(Debug)]
pub struct XWhat {
    pub text: String,
    pub leakedbytes: u64,
    pub leakedblocks: u64,
}

#[derive(Default, Debug)]
pub struct Stack {
    pub frames: Vec<Frame>,
}

#[derive(Default, Debug)]
pub struct Frame {
    pub ip: String,
    pub obj: Option<String>,
    pub func: Option<String>,
    pub dir: Option<String>,
    pub file: Option<String>,
    pub line: Option<u32>,
}

fn parse_text(reader: &mut Reader<&[u8]>, buf: &mut Vec<u8>) -> String {
    buf.clear();
    match reader.read_event_into(buf) {
        Ok(Event::Text(e)) => e.decode().unwrap_or_default().trim().to_string(),
        _ => String::new(),
    }
}

fn parse_frame(reader: &mut Reader<&[u8]>) -> Frame {
    let mut frame = Frame::default();
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let text = parse_text(reader, &mut buf);
                match tag.as_str() {
                    "ip"   => frame.ip = text,
                    "obj"  => frame.obj = Some(text),
                    "fn"   => frame.func = Some(text),
                    "dir"  => frame.dir = Some(text),
                    "file" => frame.file = Some(text),
                    "line" => frame.line = text.parse().ok(),
                    _ => {}
                }
            }
            Ok(Event::End(e))
                if String::from_utf8_lossy(e.name().as_ref()) == "frame" => break,
            Ok(Event::Eof) => break,
            _ => {}
        }
    }
    frame
}

fn parse_stack(reader: &mut Reader<&[u8]>) -> Stack {
    let mut stack = Stack::default();
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e))
                if String::from_utf8_lossy(e.name().as_ref()) == "frame" =>
            {
                stack.frames.push(parse_frame(reader));
            }
            Ok(Event::End(e))
                if String::from_utf8_lossy(e.name().as_ref()) == "stack" => break,
            Ok(Event::Eof) => break,
            _ => {}
        }
    }
    stack
}

fn parse_error(reader: &mut Reader<&[u8]>) -> Error {
    let mut error = Error::default();
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match tag.as_str() {
                    "stack" => error.stacks.push(parse_stack(reader)),
                    _ => {
                        let text = parse_text(reader, &mut buf);
                        match tag.as_str() {
                            "unique"   => error.unique = text,
                            "tid"      => error.tid = text.parse().unwrap_or(0),
                            "kind"     => error.kind = text,
                            "what"     => error.what = Some(text),
                            "auxwhat"  => error.auxwhat.push(text),
                            _ => {}
                        }
                    }
                }
            }
            Ok(Event::End(e))
                if String::from_utf8_lossy(e.name().as_ref()) == "error" => break,
            Ok(Event::Eof) => break,
            _ => {}
        }
    }
    error
}

pub fn parse(xml: &str) -> Vec<Error> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut errors = Vec::new();

    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e))
                if String::from_utf8_lossy(e.name().as_ref()) == "error" =>
            {
                errors.push(parse_error(&mut reader));
            }
            Ok(Event::Eof) => break,
            _ => {}
        }
    }
    errors
}

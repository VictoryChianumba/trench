use std::io::{self, Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

struct Inner {
  data: Vec<u8>,
  done: bool,
}

/// Reader end — implements `Read + Seek`, blocks until data is available.
pub struct StreamBuffer {
  inner: Arc<Mutex<Inner>>,
  pos: usize,
}

/// Writer end — pushed from the network thread.
pub struct StreamWriter {
  inner: Arc<Mutex<Inner>>,
}

impl StreamBuffer {
  pub fn new() -> (Self, StreamWriter) {
    let inner = Arc::new(Mutex::new(Inner { data: Vec::new(), done: false }));
    (StreamBuffer { inner: Arc::clone(&inner), pos: 0 }, StreamWriter { inner })
  }

  pub fn buffered_len(&self) -> usize {
    self.inner.lock().unwrap().data.len()
  }

  pub fn is_done(&self) -> bool {
    self.inner.lock().unwrap().done
  }
}

impl StreamWriter {
  pub fn push(&self, chunk: &[u8]) {
    self.inner.lock().unwrap().data.extend_from_slice(chunk);
  }

  pub fn finish(self) {
    self.inner.lock().unwrap().done = true;
  }
}

impl Read for StreamBuffer {
  fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
    loop {
      {
        let inner = self.inner.lock().unwrap();
        let available = inner.data.len().saturating_sub(self.pos);
        if available > 0 {
          let n = available.min(buf.len());
          buf[..n].copy_from_slice(&inner.data[self.pos..self.pos + n]);
          drop(inner);
          self.pos += n;
          return Ok(n);
        }
        if inner.done {
          return Ok(0);
        }
      }
      thread::sleep(Duration::from_millis(5));
    }
  }
}

impl Seek for StreamBuffer {
  fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
    let (len, done) = {
      let inner = self.inner.lock().unwrap();
      (inner.data.len(), inner.done)
    };

    let new_pos: i64 = match from {
      SeekFrom::Start(n) => n as i64,
      SeekFrom::Current(n) => self.pos as i64 + n,
      SeekFrom::End(n) => {
        if done {
          len as i64 + n
        } else {
          // Symphonia asks this to detect VBR headers; return unsupported
          // so it falls back to CBR assumptions — audio still plays fine.
          return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "cannot seek from end of an incomplete stream",
          ));
        }
      }
    };

    if new_pos < 0 {
      return Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "seek before start of stream",
      ));
    }

    let new_pos = new_pos as usize;

    // Forward seek past what we have: block until bytes arrive
    loop {
      let inner = self.inner.lock().unwrap();
      if inner.data.len() >= new_pos || inner.done {
        break;
      }
      drop(inner);
      thread::sleep(Duration::from_millis(5));
    }

    self.pos = new_pos;
    Ok(self.pos as u64)
  }
}

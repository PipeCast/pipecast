use futures::{SinkExt, StreamExt, TryStreamExt};
use interprocess::local_socket::tokio::prelude::LocalSocketStream;
use interprocess::local_socket::tokio::{RecvHalf, SendHalf};
use interprocess::local_socket::traits::tokio::Stream;
use serde::{Deserialize, Serialize};
use std::io::Error;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio_serde::formats::SymmetricalJson;
use tokio_serde::SymmetricallyFramed;
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

#[derive(Debug)]
pub struct Socket<In, Out> {
    address: SocketAddr,
    reader:
        SymmetricallyFramed<FramedRead<RecvHalf, LengthDelimitedCodec>, In, SymmetricalJson<In>>,
    writer:
        SymmetricallyFramed<FramedWrite<SendHalf, LengthDelimitedCodec>, Out, SymmetricalJson<Out>>,
}

impl<In, Out> Socket<In, Out>
where
        for<'a> In: Deserialize<'a> + Unpin,
        Out: Serialize + Unpin,
{
    // This is basically identical to the existing one, except we take an interprocess LocalSocketStream instead..
    pub fn new(stream: LocalSocketStream) -> Self {
        let (stream_read, stream_write) = stream.split();
        let length_delimited_read = FramedRead::new(stream_read, LengthDelimitedCodec::new());
        let reader = SymmetricallyFramed::new(length_delimited_read, SymmetricalJson::default());

        let length_delimited_write = FramedWrite::new(stream_write, LengthDelimitedCodec::new());
        let writer = SymmetricallyFramed::new(length_delimited_write, SymmetricalJson::default());

        Self {
            address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0),
            reader,
            writer,
        }
    }

    pub async fn read(&mut self) -> Option<Result<In, Error>> {
        self.reader.next().await
    }

    pub async fn try_read(&mut self) -> Result<Option<In>, Error> {
        self.reader.try_next().await
    }

    pub async fn send(&mut self, out: Out) -> Result<(), Error> {
        self.writer.send(out).await
    }

    pub fn address(&self) -> &SocketAddr {
        &self.address
    }
}

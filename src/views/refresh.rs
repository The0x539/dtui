use tokio::sync::mpsc;
use mpsc::error::TryRecvError;
use cursive::View;

pub trait Refreshable: View {
    type Update: Send;

    fn get_receiver(&mut self) -> &mut mpsc::Receiver<Self::Update>;

    fn perform_update(&mut self, update: Self::Update);

    fn refresh(&mut self) {
        loop {
            match self.get_receiver().try_recv() {
                Ok(update) => self.perform_update(update),
                Err(TryRecvError::Empty) => break,
                Err(e) => panic!("Update channel closed: {:?}", e),
            }
        }
    }
}

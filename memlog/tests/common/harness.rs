pub struct Environment {}

pub struct LogTest {}

impl LogTest {
    pub fn get(&mut self) -> Environment {
        todo!()
    }

    pub fn run<F: FnMut() + Send + 'static + ?Sized>(self, mut fns: Vec<Box<F>>) {}
}

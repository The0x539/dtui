use cursive::view::View;

pub trait ViewFn {
    type Output;
    fn call(&mut self, view: &impl View) -> Self::Output;
}

pub trait ViewMutFn {
    type Output;
    fn call_mut(&mut self, view: &mut impl View) -> Self::Output;
}

pub trait ViewTuple {
    const LEN: usize;

    fn with_elem<F: ViewFn>(&self, index: usize, f: F) -> F::Output;
    fn with_elem_mut<F: ViewMutFn>(&mut self, index: usize, f: F) -> F::Output;

    fn with_each<F: ViewFn>(&self, f: F) -> Vec<F::Output>;
    fn with_each_mut<F: ViewMutFn>(&mut self, f: F) -> Vec<F::Output>;
}

macro_rules! tuple_impls {
    ($($len:literal $iter:ident => ($($n:tt $name:ident)+))+) => {
        $(
            impl<$($name: View),+> ViewTuple for ($($name,)+) {
                const LEN: usize = $len;

                fn with_elem<F: ViewFn>(&self, index: usize, mut f: F) -> F::Output {
                    assert!(index < Self::LEN);
                    match index {
                        $($n => f.call(&self.$n),)+
                        _ => unreachable!(),
                    }
                }

                fn with_elem_mut<F: ViewMutFn>(&mut self, index: usize, mut f: F) -> F::Output {
                    assert!(index < Self::LEN);
                    match index {
                        $($n => f.call_mut(&mut self.$n),)+
                        _ => unreachable!(),
                    }
                }

                fn with_each<F: ViewFn>(&self, mut f: F) -> Vec<F::Output> {
                    let mut outputs = Vec::with_capacity(Self::LEN);

                    $(outputs.push(f.call(&self.$n));)+

                    outputs
                }

                fn with_each_mut<F: ViewMutFn>(&mut self, mut f: F) -> Vec<F::Output> {
                    let mut outputs = Vec::with_capacity(Self::LEN);

                    $(outputs.push(f.call_mut(&mut self.$n));)+

                    outputs
                }
            }
        )+
    };
}

tuple_impls! {
    1 t1 => (0 V0)
    2 t2 => (0 V0 1 V1)
    3 t3 => (0 V0 1 V1 2 V2)
    4 t4 => (0 V0 1 V1 2 V2 3 V3)
    5 t5 => (0 V0 1 V1 2 V2 3 V3 4 V4)
    6 t6 => (0 V0 1 V1 2 V2 3 V3 4 V4 5 V5)
    7 t7 => (0 V0 1 V1 2 V2 3 V3 4 V4 5 V5 6 V6)
    8 t8 => (0 V0 1 V1 2 V2 3 V3 4 V4 5 V5 6 V6 7 V7)
}

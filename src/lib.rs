pub use rand;
use rand::{rngs::SmallRng, SeedableRng as _};
use std::{
    any::Any,
    fmt::Debug,
    marker::PhantomData,
    panic::{catch_unwind, AssertUnwindSafe},
};

pub trait Arbitrary: 'static + Clone {
    fn gen(rng: &mut SmallRng) -> Self;
}

pub trait ModelState: Arbitrary + Clone + Debug {
    type Step: Arbitrary + Clone + Debug;
    fn step(&mut self, step: Self::Step);
}

pub struct ModelChecker<M: ModelState> {
    rng: SmallRng,
    _m: PhantomData<M>,
}

impl<M: ModelState> Default for ModelChecker<M> {
    fn default() -> Self {
        Self {
            rng: SmallRng::from_entropy(),
            _m: PhantomData,
        }
    }
}

#[derive(Debug)]
pub struct FailedState<M: ModelState> {
    pub state: M,
    pub steps: Vec<M::Step>,
    pub error: String,
}

impl<M: ModelState> ModelChecker<M> {
    pub fn run(&mut self, max_steps: usize) -> Result<(), FailedState<M>> {
        let state = M::gen(&mut self.rng);
        let mut steps: Vec<M::Step> = (0..max_steps)
            .map(|_| M::Step::gen(&mut self.rng))
            .collect();

        let result = Self::run_steps(state.clone(), &steps);
        let (mut last_error, failed_step) = match result {
            Ok(()) => return Ok(()),
            Err((error, failed_step)) => (error, failed_step),
        };

        // shrink steps
        steps.truncate(failed_step + 1);
        assert!(!steps.is_empty());
        let mut index = 0;
        for _ in 0..steps.len() {
            let mut shrink_steps = steps.clone();
            shrink_steps.remove(index);
            match Self::run_steps(state.clone(), &shrink_steps) {
                Ok(()) => {
                    index += 1;
                    continue;
                }
                Err((error, _)) => {
                    last_error = error;
                    steps = shrink_steps;
                }
            };
        }

        Err(FailedState {
            state,
            steps,
            error: last_error,
        })
    }

    fn run_steps(mut state: M, steps: &[M::Step]) -> Result<(), (String, usize)> {
        let mut last_step = 0;
        catch_unwind(AssertUnwindSafe(|| {
            for step in steps {
                last_step += 1;
                state.step(step.clone());
            }
        }))
        .map_err(Self::extract_panic_payload)
        .map_err(|error| (error, last_step))
    }

    fn extract_panic_payload(err: Box<dyn Any + Send>) -> String {
        if let Some(&s) = err.downcast_ref::<&str>() {
            s.to_owned()
        } else if let Some(s) = err.downcast_ref::<String>() {
            s.to_owned()
        } else {
            "UNABLE TO SHOW RESULT OF PANIC.".to_owned()
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use rand::Rng as _;

    #[derive(Clone, Debug)]
    struct TestModel;
    #[derive(Clone, Debug)]
    struct TestStep(bool);
    impl Arbitrary for TestModel {
        fn gen(_: &mut SmallRng) -> Self {
            Self
        }
    }
    impl Arbitrary for TestStep {
        fn gen(rng: &mut SmallRng) -> Self {
            Self(rng.gen_bool(0.5))
        }
    }
    impl ModelState for TestModel {
        type Step = TestStep;
        fn step(&mut self, step: Self::Step) {
            assert!(step.0);
        }
    }

    #[test]
    fn example() {
        let mut checker = ModelChecker::<TestModel>::default();
        for _ in 0..10 {
            let result = checker.run(3);
            println!("{:#?}", result);
            assert!(result
                .map(|_| true)
                .unwrap_or_else(|fail| { fail.steps.iter().filter(|step| !step.0).count() == 1 }));
        }
    }
}

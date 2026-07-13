use gazelle_macros::gazelle;

gazelle! {
    pub grammar simple {
        start s;
        terminals { A }
        s = A => make_unit;
    }
}

pub struct Actions;

impl gazelle::ErrorType for Actions {
    type Error = core::convert::Infallible;
}

impl simple::Types for Actions {
    type S = ();
}

impl gazelle::Action<simple::S<Self>> for Actions {
    fn build(&mut self, _node: simple::S<Self>) -> Result<(), Self::Error> {
        Ok(())
    }
}

pub fn parse_one() {
    let parser = simple::Parser::<Actions>::new();
    let mut actions = Actions;
    let parser = parser.push(simple::Terminal::A, &mut actions).unwrap();
    parser.finish(&mut actions).unwrap();
}

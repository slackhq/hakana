#[test]
fn simplify() {
    let mut formula = vec![];

    let mut possibilities = BTreeMap::new();

    possibilities.insert("$a".to_string(), vec![Assertion::Truthy]);
    possibilities.insert("$b".to_string(), vec![Assertion::Truthy]);

    formula.push(Clause::new(&possibilities, 1, 1, None, None, None, None));

    let mut possibilities = BTreeMap::new();

    possibilities.insert("$a".to_string(), vec![Assertion::Falsy]);

    formula.push(Clause::new(&possibilities, 2, 2, None, None, None, None));

    let simplified = simplify_cnf(formula);

    assert_eq!(2, simplified.len());
    println!("{}", simplified.get(0).unwrap().to_string());
    println!("{}", simplified.get(1).unwrap().to_string());
}

#[test]
fn combine_ored_clauses() {
    let mut formula_1 = vec![];

    let mut possibilities = BTreeMap::new();

    possibilities.insert("$a".to_string(), vec![Assertion::Truthy]);

    formula_1.push(Clause::new(&possibilities, 1, 1, None, None, None, None));

    let mut possibilities = BTreeMap::new();

    possibilities.insert("$b".to_string(), vec![Assertion::Truthy]);

    formula_1.push(Clause::new(&possibilities, 1, 1, None, None, None, None));

    let mut formula_2 = vec![];
    let mut possibilities = BTreeMap::new();

    possibilities.insert("$a".to_string(), vec![Assertion::Falsy]);

    formula_2.push(Clause::new(&possibilities, 1, 1, None, None, None, None));

    let mut possibilities = BTreeMap::new();

    possibilities.insert("$c".to_string(), vec![Assertion::Truthy]);

    formula_2.push(Clause::new(&possibilities, 1, 1, None, None, None, None));

    let combined = crate::combine_ored_clauses(&formula_1, &formula_2, 1);

    let simplified = crate::simplify_cnf(combined);

    assert_eq!(2, simplified.len());
    println!("{}", simplified.get(0).unwrap().to_string());
    println!("{}", simplified.get(1).unwrap().to_string());
}

#[test]
fn negate_clauses() {
    let mut formula = vec![];

    let mut possibilities = BTreeMap::new();

    possibilities.insert(
        "$a".to_string(),
        vec![Assertion::IsType(TAtomic::TNamedObject {
            name: "Foo".to_string(),
            type_params: None,
            is_this: false,
            extra_types: None,
            remapped_params: false,
        })],
    );
    possibilities.insert(
        "$b".to_string(),
        vec![Assertion::IsType(TAtomic::TNamedObject {
            name: "Bar".to_string(),
            type_params: None,
            is_this: false,
            extra_types: None,
            remapped_params: false,
        })],
    );

    formula.push(Clause::new(&possibilities, 1, 1, None, None, None, None));

    let mut possibilities = BTreeMap::new();

    possibilities.insert(
        "$c".to_string(),
        vec![Assertion::IsType(TAtomic::TNamedObject {
            name: "Baz".to_string(),
            type_params: None,
            is_this: false,
            extra_types: None,
            remapped_params: false,
        })],
    );
    possibilities.insert(
        "$d".to_string(),
        vec![Assertion::IsType(TAtomic::TNamedObject {
            name: "Bat".to_string(),
            type_params: None,
            is_this: false,
            extra_types: None,
            remapped_params: false,
        })],
    );

    formula.push(Clause::new(&possibilities, 1, 1, None, None, None, None));

    let negated = crate::negate_formula(&formula);

    assert!(negated.is_ok());

    let negated = negated.unwrap();

    assert_eq!(4, negated.len());
}

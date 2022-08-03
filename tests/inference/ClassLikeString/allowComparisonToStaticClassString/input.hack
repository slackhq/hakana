class A {
    const CLASSES = dict["foobar" =>  B::class];

    function foo(): bool {
        return self::CLASSES["foobar"] === static::class;
    }
}

class B extends A {}
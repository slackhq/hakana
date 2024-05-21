abstract class A {
    const CLASSES = dict["foobar" =>  B::class];

    public function foo(): bool {
        return self::CLASSES["foobar"] === static::class;
    }
}

final class B extends A {}
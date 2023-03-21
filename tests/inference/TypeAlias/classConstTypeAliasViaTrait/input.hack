abstract class A {
    abstract const type T;
}

abstract class B extends A {
    public static function getValue(): this::T {
        /* HAKANA_FIXME[InvalidReturnStatement] */
        return vec[];
    }
}

trait HasTheType {
    const type T = vec<Exception>;
}

final class C extends B {
    use HasTheType;
}

function foo(C $c): void {
    $value = C::getValue();
    foreach ($value as $v) {
        echo $v;
    }
}
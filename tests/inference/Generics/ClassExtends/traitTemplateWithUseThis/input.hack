trait HasValue<T> {
    public function get(): T {
        return static::getInner();
    }

    abstract protected static function getInner(): T;
}

abstract class A {
    use HasValue<this>;
}

final class B extends A {  
    <<__Override>>
    protected static function getInner(): B {
        return new B();
    }
}

function returnsB(B $b): B {
    return $b->get();
}
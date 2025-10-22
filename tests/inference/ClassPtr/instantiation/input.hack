<<__Sealed(B::class)>>
class A {
    public function __construct(private string $s) {}
}

final class B extends A {
}

final class C {
    public function __construct(private A $object) {}
}

function class_ptr_a(class<A> $cls): A {
    return new $cls('foo');
}

function class_ptr_b(class<B> $cls): B {
    return new $cls('foo');
}

function class_ptr_c(class<C> $cls): C {
    return new $cls('foo');
}

// type error if class_pointer_ban_classname_new=true
function classname_a(classname<A> $cls): A {
    return new $cls('foo');
}
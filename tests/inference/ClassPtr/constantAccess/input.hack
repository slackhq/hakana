abstract class A {
    const FOO = 5;
}

final class B extends A {
    const BAR = 6;
}

final class C {}

function class_ptr_a(class<A> $cls): int {
    // A::BAR is not defined
    return $cls::FOO + $cls::BAR;
}

function class_ptr_b(class<B> $cls): int {
    // valid
    return $cls::FOO + $cls::BAR;
}

function class_ptr_c(class<C> $cls): int {
    return $cls::FOO;
}

// type error if class_pointer_ban_classname_class_const=true
function classname_a(classname<A> $cls): int {
    return $cls::FOO;
}
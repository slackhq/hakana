abstract class A {
    const int FOO = 5;
}

final class B extends A {
    const int BAR = 6;
}

final class C {}

function class_ptr_a(class<A> $cls): int {
    return $cls::FOO + $cls::BAR;
}

function class_ptr_b(class<B> $cls): int {
    return $cls::FOO + $cls::BAR;
}

function class_ptr_c(class<C> $cls): int {
    return $cls::FOO;
}

// type error if class_pointer_ban_classname_class_const=true
function classname_a(classname<A> $cls): int {
    return $cls::FOO;
}
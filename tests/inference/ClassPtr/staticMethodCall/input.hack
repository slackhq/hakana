abstract class A {
    public static function foo(): void {}
}

final class B extends A {
    public static function bar(): void {}
}

final class C {}

function class_ptr_a(class<A> $cls): void {
    $cls::foo();
    $cls::bar();
}

function class_ptr_b(class<B> $cls): void {
    $cls::foo();
    $cls::bar();
}

function class_ptr_c(class<C> $cls): void {
    $cls::foo();
}

// type error if class_pointer_ban_classname_static_meth=true
function classname_a(classname<A> $cls): void {
    $cls::foo();
}
final class Foo {}

function a(): void {
    new Foo();
}

function b(): void {
    echo Foo::B1;
}

function c(): void {
    (Foo::b2<>)();
}

function d(Foo $f): void {
    $f::B3;
}

function e(Foo $f): void {
    echo $f::B4;
}

function f(Foo $f): void {
    $f::b5();
}

function g(Foo $f): void {
    $f->b6();
}

function h(Foo $f): void {
    echo $f->b7;
}

function i(Foo $f): void {
    echo $f::$b8;
}

function j(): void {
    echo Foo::$b9;
}

function k(Foo $f): void {
    $f->b10 = 5;
}

function l(Foo $f): void {
    Foo::$b11 = 5;
}
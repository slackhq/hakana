class Foo {}

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

function d(Foo $f): void {
    echo $f::B4;
}

function e(Foo $f): void {
    $f::b5();
}

function f(Foo $f): void {
    $f->b6();
}

function g(Foo $f): void {
    echo $f->b7;
}

function h(Foo $f): void {
    echo $f::$b8;
}

function i(): void {
    echo Foo::$b9;
}

function j(Foo $f): void {
    $f->b10 = 5;
}

function k(Foo $f): void {
    Foo::$b11 = 5;
}
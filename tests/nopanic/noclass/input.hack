function get_foo(): Foo {
    return new Foo();
}

function b(): void {
    echo Foo::B1;
}

function c(Foo $f): void {}

function d(): void {
    $f = get_foo();
    echo $f::class;
}

function e(): void {
    $f = get_foo();
    echo $f::B2;
}

function f(): void {
    $f = get_foo();
    $f::b3();
}

function g(): void {
    $f = get_foo();
    $f->b4();
}

function h(): void {
    $f = get_foo();
    echo $f->b5;
}

function i(): void {
    $f = get_foo();
    echo $f::$b6;
}

function j(): void {
    echo Foo::$b7;
}

function k(): void {
    $f = get_foo();
    $f->b8 = 5;
}

function l(): void {
    Foo::$b9 = 5;
}
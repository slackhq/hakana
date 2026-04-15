function takesString(string $s): void {}

enum Foo: string {
    A = "a";
    B = "b";
}

enum Bar: string as string {
    A = "a";
    B = "b";
}

enum BarInt: int as int {
    A = 1;
    B = 2;
}

function callIt(): void {
    takesString(Foo::A);
    takesString(Foo::A as string);
    echo Bar::A as string;
    echo BarInt::A as int;
    
    takesFoo("c" as Bar);
    takesBarInt(999 as BarInt);
    takesFoo(Foo::A as Foo);

    // valid
    takesFoo(Foo::A);
    takesFoo('a' as Foo);
    takesBar(Bar::A);
    takesBar("b" as Bar);
    takesBarInt(BarInt::A);
    takesBarInt(1 as BarInt);
}

function takesFoo(Foo $foo): void {}

function takesBar(Bar $bar): void {}

function takesBarInt(BarInt $bar): void {}

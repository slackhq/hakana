function takesString(string $s) {}

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
}
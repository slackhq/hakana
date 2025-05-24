function takesString(string $s) {}

enum Foo: string {
    A = "a";
    B = "b";
}

enum Bar: string as string {
    A = "a";
    B = "b";
}

function callIt(): void {
    takesString(Foo::A);
    takesString(Foo::A as string);
    echo Bar::A as string;
}
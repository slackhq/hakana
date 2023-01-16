function takesString(string $s) {}

enum Foo: string {
    A = "a";
    B = "b";
}

function callIt(): void {
    takesString(Foo::A);
    takesString(Foo::A as string);
}
abstract class A {}

class B extends A {}

function foo(): void {}

<<__EntryPoint>>
function main(): void {
    new B();
    foo();
}
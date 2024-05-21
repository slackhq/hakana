abstract class A {}

final class B extends A {}

function foo(): void {}

<<__EntryPoint>>
function main(): void {
    new B();
    foo();
}
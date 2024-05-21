abstract class A {}
final class B extends A{}

function fooFoo(B $b): void {}
function bar(A $a): void {
    fooFoo($a);
}
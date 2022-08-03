interface A {}
interface B extends A {}

function foo(B $a): A {
    return $a;
}
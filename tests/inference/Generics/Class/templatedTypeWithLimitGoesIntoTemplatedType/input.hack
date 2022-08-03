abstract class A<T> {}

function takesA(A $a) : void {}

function foo(A $a) : void {
    takesA($a);
}
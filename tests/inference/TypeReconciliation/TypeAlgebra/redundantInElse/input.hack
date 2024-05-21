abstract class A {}
final class AChild extends A {}

function foo(A $a, A $b): void {
    if ($a is AChild) {
    
    } else {
    	if ($b is AChild && !($a is AChild)) {}
    }
}
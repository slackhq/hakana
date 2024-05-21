abstract class A {}
final class B extends A {}
final class C extends A {}


function foo(A $first, A $second): void {
    $b = null;

    foreach (vec[$first, $second] as $a) {
        if ($a is B) {

        } else if (!$a is C) {
            return;
        }

        if ($b is C) {
            return;
        }

        $b = $a;
    }
}

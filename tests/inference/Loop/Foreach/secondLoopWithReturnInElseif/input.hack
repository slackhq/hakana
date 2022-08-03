class A {}
class B extends A {}
class C extends A {}

$b = null;

foreach (vec[new A, new A] as $a) {
    if ($a is B) {

    } else if (!$a is C) {
        return "goodbye";
    }

    if ($b is C) {
        return "hello";
    }

    $b = $a;
}
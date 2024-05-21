function logic(Foo $a, Foo $b) : void {
    if ((!$a is Bat || !$b is Bat)
        && (!$a is Bat || !$b is Bar)
        && (!$a is Bar || !$b is Bat)
        && (!$a is Bar || !$b is Bar)
    ) {

    } else {
        if ($b is Bat) {}
    }
}

abstract class Foo {}
final class Bar extends Foo {}
final class Bat extends Foo {}
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

class Foo {}
class Bar extends Foo {}
class Bat extends Foo {}
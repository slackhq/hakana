abstract class Foo {}
abstract class FooBar extends Foo{}
final class FooBarBat extends FooBar{}
final class FooMoo extends Foo{}

function main(Foo $a): void {
    if ($a is FooBar && !$a is FooBarBat) {

    } else if ($a is FooMoo) {

    }
}

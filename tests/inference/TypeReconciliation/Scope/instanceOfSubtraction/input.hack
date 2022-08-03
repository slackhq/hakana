class Foo {}
class FooBar extends Foo{}
class FooBarBat extends FooBar{}
class FooMoo extends Foo{}

$a = new Foo();

if ($a is FooBar && !$a is FooBarBat) {

} else if ($a is FooMoo) {

}
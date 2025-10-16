namespace Foo;

final class A {}

const keyset<classname<A>> CLASS_KEYSET  = keyset[
    A::class
];

const dict<classname<A>, int> CLASS_DICT = dict[
    A::class => 5,
];
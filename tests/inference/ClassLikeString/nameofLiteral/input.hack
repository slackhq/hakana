abstract class Base {}

abstract class Mid extends Base {}

final class A extends Base {}

final class B extends Mid {}

function foo(): bool {
    $classes = vec[A::class, B::class];
    $map = dict[ nameof B => 'test' ];
    foreach ($classes as $class) {
        $class_name = \HH\class_to_classname($class);
        if (\HH\Lib\C\contains_key($map, $class_name)) {
            return true;
        }
    }

    return false;
}

function bar(): bool {
    $classes = vec[A::class];
    $no_overlap = dict[ nameof B => 'test' ];
    foreach ($classes as $class) {
        $class_name = \HH\class_to_classname($class);
        if (\HH\Lib\C\contains_key($no_overlap, $class_name)) {
            return true;
        }
    }

    return false;
}

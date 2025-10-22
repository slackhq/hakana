final class A {}

function foo(vec<class<A>> $classes): dict<classname<A>, int> {
    $i = 0;
    $ret = dict[];
    foreach ($classes as $cls) {
        // type error if class_pointer_ban_class_array_key
        $ret[$cls] = $i++;
    }

    return $ret;
}

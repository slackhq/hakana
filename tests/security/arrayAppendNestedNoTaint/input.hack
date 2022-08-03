function foo() {
    $foo = shape(
        'a' => vec[shape('c' => 'hello')],
        'b' => vec[],
    );

    $foo['b'][] = shape(
        'c' => $_GET['bad'],
    );

    bar($foo['a']);
}

function bar(vec<shape('c' => string)> $arr): void {
    foreach ($arr as $s) {
        echo $s['c'];
    }
}
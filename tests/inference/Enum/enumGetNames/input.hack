enum MyEnum: string as string {
    FOO = 'foo';
    BAR = 'bar';
}

function foo(): string {
    $acc = '';
    $names = MyEnum::getNames();
    foreach ($names as $key => $_) {
        $acc .= $key;
    }
    return $acc;
}

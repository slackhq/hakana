function foo(): dict<arrakey, mixed> {
	$a = bar();
    $b = $a['b'] as nonnull;
    echo $b['c'];
    return $b;
}

function bar() {}

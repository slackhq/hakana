function foo(dict<string, string> $dict): dict<string, string> {
    return Dict\map(
        $dict,
        ($v) ==> {
            return $v . $v;
        }
    );
}

function bar(): void {
    $bad = dict['a' => HH\global_get('_GET')['bad']];

    $bad = foo($bad);

    foreach ($bad as $bad_value) {
        echo $bad_value;
    }
}
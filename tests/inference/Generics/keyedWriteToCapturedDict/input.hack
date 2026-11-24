function counts_in_closure(bool $a, bool $b): int {
    $reads = new HH\Lib\Ref(dict['replica' => 0, 'primary' => 0, 'failed' => 0]);
    $f = () ==> {
        if ($a) {
            $reads->value['replica']++;
            return false;
        }
        if ($b) {
            $reads->value['primary']++;
        }
        $reads->value['failed']++;
        return true;
    };
    $f();
    return $reads->value['replica'] + $reads->value['primary'] + $reads->value['failed'];
}

function counts_inline(bool $a): int {
    $reads = new HH\Lib\Ref(dict['replica' => 0, 'primary' => 0]);
    if ($a) {
        $reads->value['replica'] = 1;
    }
    return $reads->value['primary'];
}

function copied_out(): int {
    $reads = new HH\Lib\Ref(dict['replica' => 0, 'primary' => 0]);
    $counts = $reads->value;
    $counts['replica'] = 1;
    return $counts['primary'];
}

function unknown_key(string $key): int {
    $reads = new HH\Lib\Ref(dict['replica' => 0, 'primary' => 0]);
    $reads->value[$key] = 1;
    return $reads->value['primary'];
}

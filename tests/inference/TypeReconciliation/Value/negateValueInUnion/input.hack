function f(): int {
    $ret = 0;
    for ($i = 20; $i >= 0; $i--) {
        $ret = ($ret === 10) ? 1 : $ret + 1;
    }
    return $ret;
}
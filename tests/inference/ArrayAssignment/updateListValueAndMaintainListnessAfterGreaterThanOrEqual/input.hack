function takesList(vec<int> $l): vec<int> {
    if (HH\Lib\C\count($l) < 2) {
        throw new \Exception("bad");
    }

    $l[1] = $l[1] + 1;

    return $l;
}
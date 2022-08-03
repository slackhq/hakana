$arr = dict[];

foreach (vec[0, 1, 2, 3] as $i) {
    $a = rand(0, 1) ? 5 : "010";

    if (!isset($arr[(int) $a])) {
        $arr[(int) $a] = 5;
    } else {
        $arr[(int) $a] += 4;
    }
}
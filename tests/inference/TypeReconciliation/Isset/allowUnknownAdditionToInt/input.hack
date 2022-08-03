$arr = vec[1, 1, 1, 1, 2, 5, 3, 2];
$cumulative = dict[];

foreach ($arr as $val) {
    if (isset($cumulative[$val])) {
        $cumulative[$val] = $cumulative[$val] + 1;
    } else {
        $cumulative[$val] = 1;
    }
}
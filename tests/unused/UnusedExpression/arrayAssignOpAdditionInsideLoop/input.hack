function parp(dict<string, string> $arr0, dict<string, string> $arr1, dict<string, string> $arr2): void {
    $arr3 = $arr0;

    foreach ($arr1 as $a) {
        echo $a;
        $arr3 += $arr2;
    }

    if ($arr3) {}
}
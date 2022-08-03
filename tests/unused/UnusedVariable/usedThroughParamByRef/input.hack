$arr = vec[];

$populator = (inout vec<int> $arr): void ==> {
    $arr[] = 5;
};

$populator(inout $arr);

print_r($arr);
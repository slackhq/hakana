$arr = vec[vec[1, 2, 3], null, vec[1, 2, 3], null];
$b = rand(0, 2);
$c = rand(0, 2);
if (isset($arr[$b][$c])) {
    echo $arr[$b][$c];
}
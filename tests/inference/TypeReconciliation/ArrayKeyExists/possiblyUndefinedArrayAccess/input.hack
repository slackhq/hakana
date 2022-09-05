if (rand(0,1)) {
  $a = dict["a" =>  1];
} else {
  $a = vec[2, 3];
}

if (HH\Lib\C\contains_key($a, 0)) {
    echo $a[0];
}
if (rand(0,1)) {
  $a = dict["a" =>  1];
} else {
  $a = vec[2, 3];
}

if (array_key_exists(0, $a)) {
    echo $a[0];
}
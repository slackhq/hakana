$a = vec[];
$b = rand(0, 10) > 5;

foreach (vec[1, 2, 3] as $i) {
  if (rand(0, 5)) {
    $a[] = 5;
    continue;
  }

  if ($b) {
    continue; // if this is removed, no failure
  } else {} // if else is removed, no failure
}

if ($a) {}
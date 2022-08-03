$e = rand(0, 10)
  ? new RuntimeException("m")
  : null;

if ($e is Exception) {
  echo "good";
}
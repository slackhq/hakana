class Vehicle {}

function transport(mixed $m): Vehicle {
  $m as Vehicle;
  return $m;
}
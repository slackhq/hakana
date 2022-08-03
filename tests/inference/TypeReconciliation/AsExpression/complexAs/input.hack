class Vehicle {}
class Car extends Vehicle {}
class Boat extends Vehicle {}

function transport(mixed $m): ?Boat {
  // Exception if not a Vehicle.
  $v = $m as Vehicle;

  if ($v is Car) {
    // do something
    return null;
  } else {
    // Exception if $v is not a Boat.
    $v as Boat;
    return $v;
  }
}
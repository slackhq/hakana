type team_t = shape(
    "id" => int,
    ?"name" => string
);

function takesTeam(team_t $t): void {}

function givesTeam(): ?team_t {
  return shape("id" => 5, "name" => "hello");
}

function uses(): void {
  $team = givesTeam();
  $name = rand(0, 1) ? givesTeam()["name"] ?? null : null;

  if ($name is null) {}
  if ($name is string) {}

  if ($team === null) {
    return;
  }
  takesTeam($team);
}

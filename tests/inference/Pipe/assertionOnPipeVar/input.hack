function foo(?string $str): string {
    return $str |> $$ is string ? $$ : '';
}
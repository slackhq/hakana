function foo(): int {
    return vec[2,1,3]
        |> dict["a" => $$[0], "b" => $$[1], "c" => $$[2]]
        |> $$["a"];
}
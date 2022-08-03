<<\Hakana\SecurityAnalysis\ShapeSource(
	dict["email" => "pii"],
)>>
type user_t = shape(
    'id' => int,
    'email' => string,
);

function takesUser(vec<user_t> $users) {
    foreach ($users as $user) {
        echo $user["email"];
    }
}
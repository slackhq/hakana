<<\Hakana\SecurityAnalysis\ShapeSource(
	dict['email' => 'UserPassword'],
)>>
type user_t = shape(
    'id' => string,
    'email' => string,
);

function takesUser(vec<user_t> $users) {
    foreach ($users as $user) {
        echo $user['id'];
    }
}
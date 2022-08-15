<<\Hakana\SecurityAnalysis\ShapeSource(
	dict['email' => 'UserSecret'],
)>>
type user_t = shape(
    'id' => int,
    'username' => string,
    'email' => string,
);

function takesUser(user_t $user) {
    echo $user['username'];
    echo $user['email'];
}
module.exports = {
	apps: [
		{
			name: "api",
			script: "./api",
			watch: false,
			env: {
				NODE_ENV: "production",
			}
		},
		{
			name: "minibot",
			script: "./minibot",
			watch: false,
			env: {
				NODE_ENV: "production",
			}
		}
	]
};
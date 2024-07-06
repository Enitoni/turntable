#!/usr/bin/env nu

const compose_file = "compose-dev.yml"

def main [...args: string] {
	match $args {
		["db", "start", ..$rest] => {
			docker compose -f $compose_file up -d ...($rest | default [])
		}
		["db", "stop", ..$rest] => {
			docker compose -f $compose_file down ...($rest | default [])
		}
		["db", "migrate", ..$rest] => {
			sqlx migrate run --source turntable-collab\migrations\ ...($rest | default [])
		}
		_ => {
			print_usage
			exit 1
		}
	}	
}

def print_usage [] {
	print "
Usage: scripts.nu <command> [...args]

Commands:
  db start - Start the database
  db stop - Stop the database
  db migrate - Run migrations
"
}

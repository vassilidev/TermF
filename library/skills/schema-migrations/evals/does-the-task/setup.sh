#!/bin/sh
set -e
mkdir -p db/migrate app/models
cat > db/migrate/20250101000000_create_users.rb <<'RUBY'
class CreateUsers < ActiveRecord::Migration[8.1]
  def change
    create_table :users do |t|
      t.string :name, null: false
      t.string :email, null: false
      t.timestamps
    end
    add_index :users, :email, unique: true
  end
end
RUBY
cat > app/models/user.rb <<'RUBY'
class User < ApplicationRecord
  validates :name, presence: true
end
RUBY

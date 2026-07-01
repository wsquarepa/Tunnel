-- Client names are user-facing labels and must be unique so the admin panel's
-- dropdowns and lists are unambiguous.
CREATE UNIQUE INDEX idx_clients_name ON clients(name);

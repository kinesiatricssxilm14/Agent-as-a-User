package docker

import (
	"context"
	"net/http"
)

// ListNetworks returns all networks.
func (c *Client) ListNetworks(ctx context.Context) ([]Network, error) {
	var out []Network
	if err := c.do(ctx, http.MethodGet, "/networks", nil, nil, &out); err != nil {
		return nil, err
	}
	return out, nil
}

// InspectNetwork returns detailed network information.
func (c *Client) InspectNetwork(ctx context.Context, id string) (*NetworkInspect, error) {
	var out NetworkInspect
	if err := c.do(ctx, http.MethodGet, "/networks/"+id, nil, nil, &out); err != nil {
		return nil, err
	}
	return &out, nil
}

// NetworkRemove removes a network.
func (c *Client) NetworkRemove(ctx context.Context, id string) error {
	return c.do(ctx, http.MethodDelete, "/networks/"+id, nil, nil, nil)
}

package docker

import (
	"context"
	"net/http"
	"net/url"
)

// ListVolumes returns all volumes.
func (c *Client) ListVolumes(ctx context.Context) ([]Volume, error) {
	var out VolumeListResponse
	if err := c.do(ctx, http.MethodGet, "/volumes", nil, nil, &out); err != nil {
		return nil, err
	}
	return out.Volumes, nil
}

// InspectVolume returns detailed volume information.
func (c *Client) InspectVolume(ctx context.Context, name string) (*Volume, error) {
	var out Volume
	if err := c.do(ctx, http.MethodGet, "/volumes/"+name, nil, nil, &out); err != nil {
		return nil, err
	}
	return &out, nil
}

// VolumeRemove removes a volume.
func (c *Client) VolumeRemove(ctx context.Context, name string, force bool) error {
	q := url.Values{}
	if force {
		q.Set("force", "1")
	}
	return c.do(ctx, http.MethodDelete, "/volumes/"+name, q, nil, nil)
}
